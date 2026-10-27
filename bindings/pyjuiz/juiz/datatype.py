"""juiz のデータ型の Python 側 (仕様 docs/spec/datatype.md §11・§20、issue #194)

juiz-idl の Python 出力 (`juiz-idl -g python`) が作る dataclass / enum.Enum は、このモジュールを使って
辞書 (仕様 §4 の表し方) との変換と型参照 (仕様 §6 の {name, fingerprint, avro}) を持つ。

- 変換器 (Converter) は型ごとに `to_value(x)` (Python の値 → 辞書の値) と `from_value(v)` (辞書の値 → Python の値) を持つ。
  `from_value` は Rust の `juiz_types::read_value` と同じ規則で読む (足りないフィールドは既定値、型にないキーは無視、
  列挙の知らない記号は既定値)。合わなければ ValueDoesNotMatchTypeError。
- 生成した型は `__juiz_type__` (型参照の dict) を持つ。`is_juiz_type` で見分ける。
- `typed_call` は生成した client stub が使う。引数を辞書にして Process を呼び、戻り値を型で読む。
"""

import base64
import binascii
import dataclasses
import enum
import json
import math

__all__ = [
    'ValueDoesNotMatchTypeError', 'NO_DEFAULT', 'OMIT',
    'Null', 'Boolean', 'Int', 'Long', 'Float', 'Double', 'String', 'Bytes', 'Any', 'Image',
    'Array', 'Map', 'Optional', 'Record', 'Enum',
    'define_record', 'define_enum', 'is_juiz_type', 'type_schema', 'converter_of', 'coerce',
    'to_value', 'record_to_value', 'record_from_value', 'enum_from_value', 'field_default', 'typed_call',
]


class ValueDoesNotMatchTypeError(TypeError):
    """辞書の値が型に合わない (Rust の ValueDoesNotMatchTypeError と同じ、仕様 §8)"""

    def __init__(self, type_name, path, reason):
        self.type_name = type_name
        self.path = path
        self.reason = reason
        super().__init__(f"value does not match type {type_name} at '{path}': {reason}")


class _Sentinel:
    def __init__(self, name):
        self._name = name

    def __repr__(self):
        return self._name


NO_DEFAULT = _Sentinel('NO_DEFAULT')
"""フィールドに既定値がないこと"""

OMIT = _Sentinel('OMIT')
"""client stub で引数を渡さないこと (呼ばれる側の既定値を使う)"""


def _describe(v):
    try:
        return json.dumps(v, ensure_ascii=False)
    except (TypeError, ValueError):
        return f'{type(v).__name__} object'


class Converter:
    """1 つの型の変換器。name は表示名 (仕様 §5.2)"""
    name = '?'

    def to_value(self, x, path=''):
        raise NotImplementedError

    def from_value(self, v, path=''):
        raise NotImplementedError

    def _mismatch(self, path, reason):
        return ValueDoesNotMatchTypeError(self.name, path or self.name, reason)

    def _expect(self, path, what, v):
        return self._mismatch(path, f'expected {what}, found {_describe(v)}')

    def __repr__(self):
        return f'<juiz type {self.name}>'


class _Null(Converter):
    name = 'null'

    def to_value(self, x, path=''):
        if x is not None:
            raise self._expect(path, 'None', x)
        return None

    def from_value(self, v, path=''):
        if v is not None:
            raise self._expect(path, 'null', v)
        return None


class _Boolean(Converter):
    name = 'boolean'

    def to_value(self, x, path=''):
        return self.from_value(x, path)

    def from_value(self, v, path=''):
        if not isinstance(v, bool):
            raise self._expect(path, 'a boolean', v)
        return v


class _Integer(Converter):
    def __init__(self, name, bits):
        self.name = name
        self._min = -(1 << (bits - 1))
        self._max = (1 << (bits - 1)) - 1

    def to_value(self, x, path=''):
        return self.from_value(x, path)

    def from_value(self, v, path=''):
        # bool は int の派生だが整数としては受け付けない。1.0 のような小数も受け付けない (仕様 §4)
        if isinstance(v, bool) or not isinstance(v, int):
            raise self._expect(path, 'an integer', v)
        if v < self._min or v > self._max:
            raise self._mismatch(path, f'{v} is out of the range of {self.name}')
        return v


class _Floating(Converter):
    def __init__(self, name, limit):
        self.name = name
        self._limit = limit

    def to_value(self, x, path=''):
        v = self.from_value(x, path)
        if math.isnan(v) or math.isinf(v):
            raise self._mismatch(path, f'{v} cannot be a dictionary value (JSON has no NaN or Infinity)')
        return v

    def from_value(self, v, path=''):
        if isinstance(v, bool) or not isinstance(v, (int, float)):
            raise self._expect(path, 'a number', v)
        v = float(v)
        if self._limit is not None and abs(v) > self._limit and not math.isinf(v):
            raise self._mismatch(path, f'{v} is out of the range of {self.name}')
        return v


class _String(Converter):
    name = 'string'

    def to_value(self, x, path=''):
        return self.from_value(x, path)

    def from_value(self, v, path=''):
        if not isinstance(v, str):
            raise self._expect(path, 'a string', v)
        return v


class _Bytes(Converter):
    """bytes は辞書では base64 (パディングあり) の文字列 (仕様 §4)"""
    name = 'bytes'

    def to_value(self, x, path=''):
        if not isinstance(x, (bytes, bytearray, memoryview)):
            raise self._expect(path, 'bytes', x)
        return base64.b64encode(bytes(x)).decode('ascii')

    def from_value(self, v, path=''):
        if isinstance(v, (bytes, bytearray)):
            return bytes(v)
        if not isinstance(v, str):
            raise self._expect(path, 'a base64 string', v)
        try:
            return base64.b64decode(v, validate=True)
        except (binascii.Error, ValueError) as e:
            raise self._mismatch(path, f'bytes must be a base64 string ({e})')


class _Any(Converter):
    """any は検査しない。生成した型の値が入っていれば辞書にする"""
    name = 'any'

    def to_value(self, x, path=''):
        return to_value(x)

    def from_value(self, v, path=''):
        return v


class _Image(Converter):
    """image は PIL.Image.Image のまま渡す (辞書には入らない、仕様 §3.3)"""
    name = 'image'

    def to_value(self, x, path=''):
        return x

    def from_value(self, v, path=''):
        return v


Null = _Null()
Boolean = _Boolean()
Int = _Integer('int', 32)
Long = _Integer('long', 64)
Float = _Floating('float', 3.4028234663852886e+38)
Double = _Floating('double', None)
String = _String()
Bytes = _Bytes()
Any = _Any()
Image = _Image()


class Array(Converter):
    def __init__(self, items: Converter):
        self.items = items
        self.name = f'array<{items.name}>'

    def to_value(self, x, path=''):
        if not isinstance(x, (list, tuple)):
            raise self._expect(path, 'a list', x)
        return [self.items.to_value(e, f'{path}[{i}]') for i, e in enumerate(x)]

    def from_value(self, v, path=''):
        if not isinstance(v, (list, tuple)):
            raise self._expect(path, 'an array', v)
        return [self.items.from_value(e, f'{path}[{i}]') for i, e in enumerate(v)]


class Map(Converter):
    def __init__(self, values: Converter):
        self.values = values
        self.name = f'map<{values.name}>'

    def to_value(self, x, path=''):
        if not isinstance(x, dict):
            raise self._expect(path, 'a dict', x)
        return {self._key(k, path): self.values.to_value(e, f'{path}.{k}') for k, e in x.items()}

    def from_value(self, v, path=''):
        if not isinstance(v, dict):
            raise self._expect(path, 'an object', v)
        return {self._key(k, path): self.values.from_value(e, f'{path}.{k}') for k, e in v.items()}

    def _key(self, k, path):
        if not isinstance(k, str):
            raise self._mismatch(path, f'map keys must be strings, found {_describe(k)}')
        return k


class Optional(Converter):
    def __init__(self, inner: Converter):
        self.inner = inner
        self.name = f'{inner.name}?'

    def to_value(self, x, path=''):
        return None if x is None else self.inner.to_value(x, path)

    def from_value(self, v, path=''):
        return None if v is None else self.inner.from_value(v, path)


class _Named(Converter):
    """record・enum。生成したクラスは後で定義されることがあるので、クラスを返す関数で受け取る"""

    def __init__(self, cls_fn):
        self._cls_fn = cls_fn
        self._cls = None

    @property
    def cls(self):
        if self._cls is None:
            self._cls = self._cls_fn() if not isinstance(self._cls_fn, type) else self._cls_fn
        return self._cls

    @property
    def name(self):
        return self.cls.__juiz_type__['name']

    def to_value(self, x, path=''):
        return coerce(self.cls, x, path).to_value()

    def from_value(self, v, path=''):
        return coerce(self.cls, v, path)


class Record(_Named):
    pass


class Enum(_Named):
    pass


def is_juiz_type(t) -> bool:
    """juiz-idl が生成した型 (record の dataclass か enum.Enum) なら True"""
    return isinstance(t, type) and isinstance(getattr(t, '__dict__', {}).get('__juiz_type__'), dict)


def type_schema(t) -> dict:
    """生成した型の型参照 {name, fingerprint, avro} (仕様 §6) の写し"""
    if not is_juiz_type(t):
        raise TypeError(f'{t!r} is not a type generated by juiz-idl')
    return json.loads(json.dumps(t.__juiz_type__))


def converter_of(t) -> Converter:
    """生成した型の変換器"""
    if isinstance(t, Converter):
        return t
    if is_juiz_type(t):
        return Enum(t) if issubclass(t, enum.Enum) else Record(t)
    raise TypeError(f'{t!r} is not a type generated by juiz-idl')


def _juiz_name_of(x):
    t = type(x)
    if is_juiz_type(t):
        return t.__juiz_type__['name'], t.__juiz_type__['fingerprint']
    return None


def coerce(cls, v, path=''):
    """生成した型 cls の値にする。

    - すでに cls の値ならそのまま (Python 同士ではネイティブ値のまま渡る、遅延評価 L1)
    - 同じ完全名・指紋の別のクラス (別の場所に生成したもの) の値や、ほかの生成した型の値は、辞書にしてから読む
    - 辞書 (やほかの言語・ブローカーから届いた値) は仕様 §4 の規則で読む
    """
    if isinstance(v, cls):
        return v
    if _juiz_name_of(v) is not None:
        v = v.to_value()
    return cls.from_value(v, path)


def to_value(x):
    """生成した型の値を辞書にする。リスト・辞書の中も見る。それ以外はそのまま"""
    if _juiz_name_of(x) is not None:
        return x.to_value()
    if isinstance(x, list):
        return [to_value(e) for e in x]
    if isinstance(x, tuple):
        return [to_value(e) for e in x]
    if isinstance(x, dict):
        return {k: to_value(e) for k, e in x.items()}
    return x


@dataclasses.dataclass
class _Field:
    name: str        # 辞書のキー (スキーマのフィールド名)
    attr: str        # Python の属性名 (キーワードなら末尾に _)
    converter: Converter
    default: object  # 辞書の表し方の既定値。なければ NO_DEFAULT


def define_record(cls, *, schema: dict, fields):
    """生成した record の dataclass に型の情報を付ける。fields は (キー, 属性名, 変換器, 既定値) の列を返す関数"""
    cls.__juiz_type__ = schema
    cls.__juiz_fields_fn__ = fields
    cls.__juiz_fields__ = None
    return cls


def define_enum(cls, *, schema: dict, default=None):
    """生成した enum.Enum に型の情報を付ける。default は既定の記号 (なければ None)"""
    cls.__juiz_type__ = schema
    cls.__juiz_default__ = default
    return cls


def _fields(cls):
    fs = cls.__dict__.get('__juiz_fields__')
    if fs is None:
        fs = [_Field(*f) for f in cls.__juiz_fields_fn__()]
        cls.__juiz_fields__ = fs
    return fs


def field_default(cls, attr):
    """フィールドの既定値を Python の値にする (dataclass の default_factory が使う)"""
    for f in _fields(cls):
        if f.attr == attr:
            return f.converter.from_value(json.loads(json.dumps(f.default)), f'{cls.__juiz_type__["name"]}.{f.name}')
    raise AttributeError(attr)


def record_to_value(obj) -> dict:
    """record の値を辞書にする。すべてのフィールドを出す (仕様 §4)"""
    cls = type(obj)
    name = cls.__juiz_type__['name']
    return {f.name: f.converter.to_value(getattr(obj, f.attr), f'{name}.{f.name}') for f in _fields(cls)}


def record_from_value(cls, value, path=''):
    """辞書を record として読む。足りないフィールドは既定値、型にないキーは無視 (仕様 §4)"""
    name = cls.__juiz_type__['name']
    path = path or name
    if isinstance(value, cls):
        return value
    if not isinstance(value, dict):
        raise ValueDoesNotMatchTypeError(name, path, f'expected an object, found {_describe(value)}')
    kwargs = {}
    for f in _fields(cls):
        fpath = f'{path}.{f.name}'
        if f.name in value:
            kwargs[f.attr] = f.converter.from_value(value[f.name], fpath)
        elif f.default is NO_DEFAULT:
            raise ValueDoesNotMatchTypeError(name, fpath, f"field '{f.name}' is missing and has no default")
        else:
            kwargs[f.attr] = f.converter.from_value(json.loads(json.dumps(f.default)), fpath)
    return cls(**kwargs)


def enum_from_value(cls, value, path=''):
    """記号の文字列を列挙として読む。知らない記号は既定値があれば既定値 (仕様 §4)"""
    name = cls.__juiz_type__['name']
    path = path or name
    if isinstance(value, cls):
        return value
    if not isinstance(value, str):
        raise ValueDoesNotMatchTypeError(name, path, f'expected a symbol string, found {_describe(value)}')
    for m in cls:
        if m.value == value:
            return m
    default = cls.__dict__.get('__juiz_default__')
    if default is not None:
        return enum_from_value(cls, default, path)
    raise ValueDoesNotMatchTypeError(name, path, f"'{value}' is not a symbol of {name}")


def typed_call(proxy, arguments, returns: Converter):
    """client stub から Process を呼ぶ。

    arguments は (引数名, 値, 変換器) の列。値が OMIT の引数は渡さない (呼ばれる側の既定値を使う)。
    proxy は juiz.proxy の ProcessProxy か ContainerProcessProxy。戻り値は returns で読む。
    """
    body = {}
    has_image = False
    for name, value, converter in arguments:
        if value is OMIT:
            continue
        v = converter.to_value(value, name)
        if converter is Image:
            has_image = True
        body[name] = v
    if has_image:
        # 画像があると multipart で送るので、ほかの引数は JSON の文字列にする (proxy の決まり)
        body = {k: (v if _is_pil_image(v) else json.dumps(v)) for k, v in body.items()}
    result = proxy.call(body=body)
    return returns.from_value(result)


def _is_pil_image(v):
    try:
        from PIL.Image import Image as PILImage
    except ImportError:
        return False
    return isinstance(v, PILImage)
