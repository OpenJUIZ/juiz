"""この機械の名前の一覧から、名前で juiz を引く (docs/spec/naming.md §9.4、issue #618)

juiz は動き続けるとき、``<dir>/names/<ネームスペース>/<名前>.json`` に自分のブローカーを書く (§3)。
pyjuiz は juiz を通さず、このファイルを直接読んで名前を解決する。

    import juiz
    from juiz.proxy import connect, ProcessProxy
    system = connect('mobile_sim')                    # この機械の mobile_sim
    system = connect('robot1/mobile_sim')             # ネームスペース robot1 の mobile_sim
    system = connect('name://mobile_sim?type=http')
    p = ProcessProxy('inc0', host='mobile_sim', protocol='name')
    juiz.resolve_name('mobile_sim')                   # 'ipc://mobile_sim'
    juiz.name_list()                                  # 登録の一覧 (juiz name list --json と同じ形)

- 名前の書き方は ``mobile_sim``・``robot1/mobile_sim``・``/mobile_sim`` (ルート)・``mobile_sim@<機械>`` (§4.2)。
- ``name://<名前>?ns=<ネームスペース>&type=<種類>`` でも書ける (§9.1)。
- pyjuiz がつなげるのは ipc と http なので、ブローカーはこの順で選ぶ (§7.3)。
- 死んだ登録 (pid がいない) は飛ばす。ファイルは消さない (消すのは juiz の役目)。
"""
import json
import os
import re

FORMAT = 1
NAMES_DIR = 'names'
IPC_DIR_ENV = 'JUIZ_IPC_DIR'

# pyjuiz がつなげる種類を、同じ機械で選ぶ順 (§7.3 の ipc → zenoh → qmp → http のうち、ipc と http)
KINDS = ('ipc', 'http')

# system/profile を待つ時間 [秒] (§6 の 2)
PROBE_TIMEOUT = 1.0

_NAME_REGEX = re.compile(r'^[A-Za-z0-9_.\-]{1,64}$')


class NameNotRegisteredError(Exception):
    """名前に当てはまる、生きている登録が無い (§7.2)"""
    pass


class NameAmbiguousError(Exception):
    """名前に当てはまる登録が 2 つ以上ある (§7.2)"""
    pass


def names_dir() -> str:
    """登録ファイルを置くディレクトリ ``<dir>/names`` (§3.1)。読むだけなので作らない"""
    d = os.environ.get(IPC_DIR_ENV)
    if not d:
        if os.name == 'nt':
            local = os.environ.get('LOCALAPPDATA', '')
            d = os.path.join(local, 'juiz', 'run')
        else:
            runtime = os.environ.get('XDG_RUNTIME_DIR')
            d = os.path.join(runtime, 'juiz') if runtime else f'/tmp/juiz-{os.getuid()}'
    return os.path.join(d, NAMES_DIR)


def _validate(part: str, what: str, s: str):
    if not _NAME_REGEX.match(part) or part.startswith('.'):
        raise ValueError(f"invalid registered name '{s}': '{part}' can not be used as {what}. "
                         "Use 1 to 64 characters of [A-Za-z0-9_-.], not starting with '.'")


def _namespace(ns: str, s: str) -> str:
    ns = ns.strip('/')
    for seg in [x for x in ns.split('/') if x != ''] if ns else []:
        _validate(seg, 'a namespace', s)
    return ns


class NameQuery(object):
    """名前の引き方 (§4.2)。namespace が None ならどのネームスペースでも、'' はルート"""

    def __init__(self, name, namespace=None, host=None, kind=None):
        self.name = name
        self.namespace = namespace
        self.host = host
        self.kind = kind

    @staticmethod
    def parse(s: str) -> 'NameQuery':
        """``mobile_sim``・``robot1/mobile_sim``・``/mobile_sim``・``mobile_sim@pc``・``name://…`` を読む"""
        if s.startswith('name://'):
            return NameQuery._parse_url(s[len('name://'):].rstrip('/'), s)
        if '://' in s or ':' in s:
            raise ValueError(f"'{s}' is not a registered name")
        return NameQuery._parse_body(s, s)

    @staticmethod
    def _parse_body(body: str, s: str) -> 'NameQuery':
        host = None
        if '@' in body:
            body, host = body.split('@', 1)
            if host == '' or '@' in host or '/' in host:
                raise ValueError(f"invalid registered name '{s}'. "
                                 "Write the machine once after the name (ex., mobile_sim@robot1-pc)")
        namespace = None
        if '/' in body:
            ns, body = body.rsplit('/', 1)
            namespace = _namespace(ns, s)
        _validate(body, 'a name', s)
        return NameQuery(body, namespace, host)

    @staticmethod
    def _parse_url(rest: str, s: str) -> 'NameQuery':
        body, _, query = rest.partition('?')
        if '/' in body:
            raise ValueError(f"invalid name '{s}'. "
                             "Write the namespace as a query (ex., name://mobile_sim?ns=robot1)")
        q = NameQuery._parse_body(body, s)
        for pair in [p for p in query.split('&') if p]:
            k, _, v = pair.partition('=')
            if k == 'ns':
                q.namespace = _namespace(v, s)
            elif k == 'type' and v:
                q.kind = v
            else:
                raise ValueError(f"unknown query '{pair}' in '{s}'. "
                                 "Only ns=<namespace> and type=<broker type> can be written")
        return q

    def matches(self, reg: dict) -> bool:
        if reg.get('name') != self.name:
            return False
        if self.namespace is not None and reg.get('namespace', '').strip('/') != self.namespace:
            return False
        if self.host is not None and reg.get('host') != self.host:
            return False
        return True

    def __str__(self):
        if self.namespace is None:
            s = self.name
        elif self.namespace == '':
            s = '/' + self.name
        else:
            s = f'{self.namespace}/{self.name}'
        if self.host is not None:
            s += '@' + self.host
        return s


def is_name(address: str) -> bool:
    """つなぎ先が名前か (``name://…`` か、``://`` も ``:`` も無いもの、§9.2)"""
    return address.startswith('name://') or ('://' not in address and ':' not in address)


def registered_name(reg: dict) -> str:
    ns = reg.get('namespace', '').strip('/')
    return f"{ns}/{reg['name']}" if ns else reg['name']


def local_url(broker: dict) -> str:
    """同じ機械からつなぐ URL。host が 0.0.0.0・::・空なら 127.0.0.1 にする (§3.3)"""
    bid = broker.get('id', '')
    scheme, sep, rest = bid.partition('://')
    if not sep or scheme == 'ipc':
        return bid
    authority, slash, path = rest.partition('/')
    host, port = authority, None
    if ':' in authority:
        h, p = authority.rsplit(':', 1)
        if p.isdigit():
            host, port = h, p
    if host in ('', '0.0.0.0', '::', '[::]'):
        host = '127.0.0.1'
    url = f'{scheme}://{host}' + (f':{port}' if port else '')
    return url + (slash + path if slash else '')


def pid_alive(pid) -> bool:
    """プロセスが生きているか (§6 の 1)"""
    if not isinstance(pid, int) or pid <= 0:
        return False
    if os.name == 'nt':
        import ctypes
        from ctypes import wintypes
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
        ERROR_INVALID_PARAMETER = 87
        STILL_ACTIVE = 259
        h = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if not h:
            return ctypes.get_last_error() != ERROR_INVALID_PARAMETER
        try:
            code = wintypes.DWORD()
            ok = kernel32.GetExitCodeProcess(h, ctypes.byref(code))
            return not (ok and code.value != STILL_ACTIVE)
        finally:
            kernel32.CloseHandle(h)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return False
    return True


def _read_all():
    d = names_dir()
    out = []
    if not os.path.isdir(d):
        return out
    for root, _dirs, files in os.walk(d):
        for f in files:
            if not f.endswith('.json'):
                continue
            try:
                with open(os.path.join(root, f), encoding='utf-8') as fp:
                    reg = json.load(fp)
            except (OSError, ValueError):
                # 書いている途中か壊れたもの
                continue
            if not isinstance(reg, dict) or reg.get('format') != FORMAT:
                continue
            if not isinstance(reg.get('name'), str) or not isinstance(reg.get('brokers', []), list):
                continue
            out.append(reg)
    return out


def _probe(broker: dict):
    """system/profile の system_store.uuid。応答が無ければ None"""
    url = local_url(broker)
    try:
        if broker.get('type_name') == 'ipc':
            from .proxy import IpcProxy
            name = url[len('ipc://'):].split('/')[0]
            with IpcProxy(name, timeout=PROBE_TIMEOUT) as p:
                profile = p.read_profile('system')
        else:
            import httpx
            r = httpx.get(url.rstrip('/') + '/api/system/profile', timeout=PROBE_TIMEOUT)
            if r.status_code != 200:
                return None
            profile = r.json()
        return profile.get('system_store', {}).get('uuid')
    except Exception:
        return None


def _alive(reg: dict) -> bool:
    """§6 の 1〜3。pyjuiz がつなげる種類で最初の 1 つずつに問い合わせる"""
    if not pid_alive(reg.get('pid')):
        return False
    brokers = reg.get('brokers', [])
    for kind in KINDS:
        b = next((b for b in brokers if b.get('type_name') == kind), None)
        if b is None:
            continue
        u = _probe(b)
        if u is not None:
            return u == reg.get('uuid')
    return False


def name_list(check: bool = False):
    """この機械の登録の一覧 (§9.3 の ``juiz name list --json`` と同じ形)。

    pid の死んだものは飛ばす。check=True なら各登録に system/profile を問い合わせ、``alive`` を付ける。
    """
    out = []
    for reg in _read_all():
        if not pid_alive(reg.get('pid')):
            continue
        reg = dict(reg)
        if check:
            reg['alive'] = _alive(reg)
        reg['machine'] = 'this'
        out.append(reg)
    out.sort(key=registered_name)
    return out


def candidates(name):
    """名前に当てはまる、生きている登録をすべて (§7.1)"""
    q = name if isinstance(name, NameQuery) else NameQuery.parse(name)
    return [r for r in _read_all() if q.matches(r) and _alive(r)]


def _select_broker(reg: dict, kind):
    has = ', '.join(dict.fromkeys(b.get('type_name', '') for b in reg.get('brokers', [])))
    if kind is not None:
        b = next((b for b in reg.get('brokers', []) if b.get('type_name') == kind), None)
        if b is None:
            raise NameNotRegisteredError(f'{registered_name(reg)} has no {kind} broker (it has: {has})')
        if kind not in KINDS:
            raise ValueError(f'pyjuiz can not connect to a {kind} broker')
        return local_url(b)
    for k in KINDS:
        b = next((b for b in reg.get('brokers', []) if b.get('type_name') == k), None)
        if b is not None:
            return local_url(b)
    raise NameNotRegisteredError(
        f'{registered_name(reg)} has no broker pyjuiz can connect to (it has: {has})')


def resolve_name(name: str) -> str:
    """名前を 1 つの登録に決め、つなぐブローカーの URL を返す (§7.2・§7.3)。

    候補が無ければ NameNotRegisteredError、2 つ以上なら NameAmbiguousError。
    """
    q = NameQuery.parse(name)
    cs = candidates(q)
    if not cs:
        raise NameNotRegisteredError(f'{q} is not registered on this machine.')
    if len(cs) > 1:
        names = ', '.join(f'{registered_name(c)} (this machine)' for c in cs)
        raise NameAmbiguousError(
            f'{q} is ambiguous: {names}. '
            f'Write the namespace or the machine (ex., {registered_name(cs[0])}).')
    return _select_broker(cs[0], q.kind)
