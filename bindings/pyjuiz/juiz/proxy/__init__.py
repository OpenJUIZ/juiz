import re
import httpx
import json, io
from PIL import Image
from io import BytesIO
from ..datatype import to_value as _to_value


class UnsupportedMediaTypeError(Exception): pass
class BadRequestError(Exception): pass
class ForbiddenError(Exception): pass
class NotFoundError(Exception): pass
class RequestTimeoutError(Exception): pass
class InternalServerError(Exception): pass
class AmbiguousNameError(Exception): pass


error_code_and_ex={
    415: UnsupportedMediaTypeError,
    400: BadRequestError,
    403: ForbiddenError,
    404: NotFoundError,
    408: RequestTimeoutError,
    500: InternalServerError,
}

class CRUDProxy():
    
    def __init__(self):
        pass
    
    def _url(self, class_name, function_name, param_dict: dict=None):
        if param_dict is None:
            return f'/api/{class_name}/{function_name}'
        else:
            return f'/api/{class_name}/{function_name}?' + '&'.join([f'{k}={v}' for k, v in param_dict.items()])
    
    def read(self, class_name, function_name, param_dict={}):
        pass
    
    def delete(self, class_name, function_name, param_dict={}):
        pass
    
    def create(self, class_name, function_name, body, param_dict={}):
        pass
    
    def update(self, class_name, function_name, body, param_dict={}):
        pass
    
class HttpProxy(CRUDProxy):
    
    def __init__(self, host, port):
        self._host = host
        self._port = port
        self._base_url = 'http://' + self._host + ':' + str(self._port)
        pass

    def _do_response(self, response):
        if response.status_code == 200:
            ct = response.headers.get('content-type', None)
            if ct and ct == 'application/json':
                return response.json()
            elif ct and ct == 'image/png':
                return Image.open(BytesIO(response.content)).convert('RGB')
        else:
            # サーバーのエラーメッセージ ({"message": ...}) を例外に載せる (issue #171)
            try:
                message = response.json().get('message', response.text)
            except Exception:
                message = response.text
            raise error_code_and_ex.get(response.status_code, InternalServerError)(message)
    
    def read(self, class_name, function_name, param_dict={}):
        url = self._url(class_name, function_name, param_dict)
        return self._get(url)
    
    def delete(self, class_name, function_name, param_dict={}):
        url = self._url(class_name, function_name, param_dict)
        return self._delete(url)
    
    def create(self, class_name, function_name, body: dict, param_dict: dict={}):
        url = self._url(class_name, function_name, param_dict)
        return self._post(url, body)
    
    def update(self, class_name, function_name, body, param_dict={}, profile=None):
        url = self._url(class_name, function_name, param_dict)
        return self._patch(url, body, profile=profile)
    
    def _get(self, url):
        with httpx.Client() as client:
            return self._do_response(client.get(self._base_url + url))                
        return None

    def _delete(self, url):
        with httpx.Client() as client:
            return self._do_response(client.delete(self._base_url + url))                
        return None

    def _post(self, url, data):
        with httpx.Client() as client:
            return self._do_response(client.post(self._base_url + url,
                                    data=json.dumps(data),
                                   headers={"Content-Type": "application/json"}
                                   ))
        return None

    def _patch(self, url, data, profile=None):
        with httpx.Client() as client:
            img_key_value = None
            serializable_data = {}
            if isinstance(data, Image.Image):
                #print('prof:', profile)
                pass
            else:
                for k, v in data.items():
                    if isinstance(v, Image.Image):
                        img_key_value = (k, v)
                    else:
                        serializable_data[k] = v
            if img_key_value is not None:
                output = io.BytesIO()
                img_key_value[1].save(output, 'PNG')
                files = {}
                for k, v in serializable_data.items():
                    files[k] = (None, v, "application/json")
                files[img_key_value[0]] = (None, output.getvalue(), "image/png")
                return self._do_response(client.put(self._base_url + url,
                            # data=json.dumps(serializable_data),
                            headers={"Content-Type": "multipart/form-data; boundary=+++"},
                            #files={img_key_value[0]: (None, output.getvalue(), "image/png")}))
                            files=files))
            else:
                return self._do_response(client.patch(self._base_url + url,
                                        data=json.dumps(data),
                                    headers={"Content-Type": "application/json"}
                                    )) 
        return None

class SystemProxy():

    def __init__(self, base_proxy: CRUDProxy):
        self.crud = base_proxy
        self._processes = None
        self._containers = None
        self._container_processes = None
        pass

    def profile_full(self, update=False):
        return self.crud.read('system', 'profile_full')

    def create_process(self, type_name, name, use_memo=True):
        return self.crud.create('process', 'create', {'__manifest__': {
            "type_name": type_name,
            "name": name,
            "use_memo": use_memo}})

    # 実行中に Composite を定義する (issue #171)。定義は conf の composites: / composite_containers: と同じ形の dict

    def create_composite(self, definition: dict):
        """Composite の Factory を登録し、Factory の manifest を返す。

        definition は {'type_name': ..., 'body': {...}} (conf の composites: の 1 件と同じ形)。
        スロット (containers) があれば CompositeContainerProcess の型になる。
        """
        return self.crud.create('composite', 'create', {'__manifest__': definition})

    def composites(self):
        """登録済みの Composite の型名のリスト (名前順)"""
        return self.crud.read('composite', 'list')

    def composite(self, type_name: str):
        """Composite の Factory の manifest (式の木は 'composition')"""
        return self.crud.read('composite', 'profile_full', {'identifier': type_name})

    def destroy_composite(self, type_name: str):
        """Composite の Factory の登録を外す。インスタンスや、子として使う Composite があればエラー"""
        return self.crud.delete('composite', 'destroy', {'identifier': type_name})

    def create_composite_container(self, definition: dict):
        """CompositeContainer とその上の Process を作り、ContainerProxy を返す。

        definition は {'name': ..., 'namespace': ..., 'slots': {...}, 'processes': [...]}
        (conf の composite_containers: の 1 件と同じ形。namespace は絶対)。
        消すときは destroy_container(ID)。
        """
        profile = self.crud.create('composite_container', 'create', {'__manifest__': definition})
        return ContainerProxy.new(self, profile['identifier'])

    def destroy_container(self, identifier: str):
        """Container (CompositeContainer を含む) を消す"""
        return self.crud.delete('container', 'destroy', {'identifier': identifier})
        
    def _list(self, class_name, namespace=None):
        # namespace を渡すと、そのネームスペースとその下だけを返す (list API の namespace クエリ)
        params = {} if namespace is None else {'namespace': namespace}
        return [to_identifier(i, class_name) for i in self.crud.read(class_name, 'list', params)]

    def processes(self, namespace=None):
        self._processes = [ProcessProxy.new(self, pid) for pid in self._list('process', namespace)]
        return self._processes.copy()

    def process(self, id):
        ps = self.crud.read('process', 'list')
        for pid in ps:
            pid = to_identifier(pid, 'process')
            if pid == id:
                return ProcessProxy.new(self, pid)
        return None            
        
    def containers(self, update=False, namespace=None):
        self._containers = [ContainerProxy.new(self, cid) for cid in self._list('container', namespace)]
        return self._containers

    def container(self, id):
        cs = self.crud.read('container', 'list')
        for pid in cs:
            pid = to_identifier(pid, 'container')
            if pid == id:
                return ContainerProxy.new(self, pid)
        return None            
    
    def container_processes(self, update=False, namespace=None):
        if namespace is not None:
            return [ContainerProcessProxy(self, p) for p in self._list('container_process', namespace)]
        if self._container_processes is None or update:
            self._container_processes = [ContainerProcessProxy(self, p) for p in self._list('container_process')]
        return self._container_processes

    
class ObjectProxy(object):
    
    def __init__(self, system_proxy, class_name, identifier):
        self.system = system_proxy
        self._id = identifier
        self._cn = class_name
        self._profile = None
        self._name = id_to_name(identifier) if identifier is not None else None
        self._namespace = None
        self._type_name = None
        
    def profile_full(self, update=False):
        if self._profile is None or update:
            self._profile = self.system.crud.read(self._cn, 'profile_full', {'identifier': self._id})
        return self._profile

    def identifier(self, update=False):
        return self._id
    
    def name(self):
        if self._name is None:
            self._name = id_to_name(self._id)
        return self._name
    
    def namespace(self):
        """ネームスペース (ルートなら空文字列)"""
        if self._namespace is None:
            self._namespace = id_to_namespace(self._id)
        return self._namespace

    def full_name(self):
        """ネームスペースを含めた名前 (<namespace>/<name>)"""
        return join_namespace(self.namespace(), self.name())

    def type_name(self):
        if self._type_name is None:
            self._type_name = id_to_type_name(self._id)
        return self._type_name
        

    def destroy(self):
        return self._system_proxy.delete('/api/' + self._class_name + '/destroy?identifier=' + self._identifier)

def _find_by_name(objs, name, namespace=None):
    """名前で探す。

    - namespace を渡すと、そのネームスペースの name だけを探す ('/' か '' でルート)。
    - name が 'robot1/inc0' のように / を含めば、ネームスペースを含めた名前として探す。
    - どちらでもなければ、ルートの name を優先し、無ければネームスペースを問わず探す。
      複数のネームスペースに同じ名前があれば AmbiguousNameError。
    見つからなければ None。
    """
    if namespace is not None or '/' in name:
        ns = (namespace or '').strip('/')
        full_name = join_namespace(ns, name).lstrip('/')
        return next((o for o in objs if o.full_name() == full_name), None)
    found = [o for o in objs if o.name() == name]
    root = [o for o in found if o.namespace() == '']
    if root:
        return root[0]
    if len(found) > 1:
        raise AmbiguousNameError(
            f'"{name}" exists in several namespaces: {sorted(o.full_name() for o in found)}. '
            'Pass namespace= or a name like "<namespace>/<name>".')
    return found[0] if found else None


class ProcessProxy(ObjectProxy):
    def __init__(self, name, host, port, protocol='http', namespace=None):
        if protocol is None:
            super().__init__(None, 'process', None)
            self._name = name
        elif protocol == 'http':
            super().__init__(SystemProxy(HttpProxy(host,port)), 'process', None)
            self._name = None
            p = _find_by_name(self.system.processes(), name, namespace)
            if p is not None:
                self._id = p.identifier()
                return
            raise NotFoundError(f'process "{join_namespace(namespace or "", name)}" not found in http://{host}:{port}')
            
    def __repr__(self):
        return f'ProcessProxy("{self.identifier()}")'
    
    @classmethod
    def new(cls, system_proxy, identifier):
        p = ProcessProxy(None, None, None, protocol=None)
        p.system = system_proxy
        p._id = identifier
        return p
    
    def call(self, *args, **kwargs):
        prof = self.profile_full()
        if len(args) == 1:
            arg_prof = prof['arguments']
            body = {
                arg_prof[0]['name']: _to_value(args[0])
            }
        elif 'body' in kwargs.keys():
            body = kwargs['body']
        else:
            body = {}
            args_prof = prof['arguments']
            for a, a_prof in zip(args, args_prof):
                if isinstance(a, Image.Image):
                    body[a_prof['name']] = a
                else:
                    body[a_prof['name']] = json.dumps(_to_value(a))
            for k, v in kwargs.items():
                body[k] = _to_value(v)
        return self.system.crud.update(self._cn, 'call', body, {'identifier': self._id}, profile=self._profile)
    
    def __call__(self, *args, **kwargs):
        return self.call(*args, **kwargs)

    def execute(self):
        return self.system.crud.update(self._cn, 'execute', {}, {'identifier': self._id})
    
    def p_apply(self, arg_name: str, value: dict): 
        return self.system.crud.update(self._cn, 'p_apply', {'arg_name': arg_name, 'value': _to_value(value)}, {'identifier': self._id})

class ContainerProxy(ObjectProxy):
        
    def __init__(self, name, host, port, protocol='http', namespace=None):
        if protocol is None:
            super().__init__(None, 'container', None)
            self._name = name
        elif protocol == 'http':
            super().__init__(SystemProxy(HttpProxy(host,port)), 'container', None)
            self._name = None
            cs = self.system.containers()
            # 名前で探し、見つからなければ従来どおり型名で探す
            c = _find_by_name(cs, name, namespace)
            if c is not None:
                self._id = c.identifier()
                return
            if namespace is None:
                for c in cs:
                    if c.type_name() == name:
                        self._id = c.identifier()
                        return
            raise NotFoundError(f'container "{join_namespace(namespace or "", name)}" not found in http://{host}:{port}')
                
    @classmethod
    def new(cls, system_proxy, identifier):
        p = ContainerProxy(None, None, None, protocol=None)
        p.system = system_proxy
        p._id = identifier
        return p
    
    def __repr__(self):
        return f'ContainerProxy("{self.identifier()}")'
    
    def processes(self):
        # Container の profile_full の processes は空のことがあるので、
        # container_process/list からこの Container に属するものを選ぶ
        # ContainerProcess は所属 Container と同じネームスペースに置かれる
        ps = self.system.crud.read('container_process', 'list')
        return [ContainerProcessProxy(self.system, to_identifier(p, 'container_process'))
                for p in ps
                if isinstance(p, dict) and p.get('container_name') == self.name()
                and (p.get('namespace') or '') == self.namespace()]
        
    def process(self, id):
        for p in self.processes():
            if p.identifier() == id:
                return p
        return None          
    
    def __getattr__(self, name):
        # search process by name:
        ps = self.processes()
        for p in ps:
            if p.type_name() == name:
                return p
        for p in ps:
            if p.name()  == name:
                return p
        raise AttributeError(self, name)
    

class ContainerProcessProxy(ObjectProxy):
    def __init__(self, system_proxy, identifier):
        super().__init__(system_proxy, 'container_process', identifier)    

    def __repr__(self):
        return f'ContainerProcess("{self.identifier()}")'

    def call(self, *args, **kwargs):
        prof = self.profile_full()
        if len(args) == 1:
            arg_prof = prof['arguments']
            body = {
                arg_prof[0]['name']: _to_value(args[0])
            }
            #body = args[0]
        elif 'body' in kwargs.keys():
            body = kwargs['body']
        else:
            body = {}
            args_prof = prof['arguments']
            for a, a_prof in zip(args, args_prof):
                if isinstance(a, Image.Image):
                    body[a_prof['name']] = a
                else:
                    body[a_prof['name']] = json.dumps(_to_value(a))
            for k, v in kwargs.items():
                body[k] = _to_value(v)
            #body = kwargs
        return self.system.crud.update(self._cn, 'call', body, {'identifier': self._id}, profile=self.profile_full())

    def __call__(self, *args, **kwargs):
        return self.call(*args, **kwargs)
    
    def execute(self):
        return self.system.crud.update(self._cn, 'execute', {}, {'identifier': self._id}, profile=self._profile)

    def p_apply(self, arg_name: str, value: dict): 
        return self.system.crud.update(self._cn, 'p_apply', {'arg_name': arg_name, 'value': _to_value(value)}, {'identifier': self._id})


class IdentifierStruct(object):

    def __init__(self, broker_type, broker_name, class_name, type_name, name, namespace=''):
        self.broker_type = broker_type
        self.broker_name = broker_name
        self.class_name = class_name
        self.type_name = type_name
        self.namespace = namespace
        self.name = name

    @classmethod
    def fromIdentifier(cls, id):
        m = _IDENTIFIER_REGEX.match(id)
        if m is None:
            raise ValueError(f'invalid identifier: {id!r}')
        broker_type, broker_name, class_name, full_name, type_name = m.groups()
        namespace, name = split_namespace(full_name)
        return IdentifierStruct(broker_type, broker_name, class_name, type_name, name, namespace)

    def full_name(self):
        """ネームスペースを含めた名前 (<namespace>/<name>)"""
        return join_namespace(self.namespace, self.name)

    def to_id(self):
        return f'{self.broker_type}://{self.broker_name}/{self.class_name}/{self.full_name()}::{self.type_name}'

    def __repr__(self):
        return f'IdentifierStruct(broker_type="{self.broker_type}",broker_name="{self.broker_name}",class_name="{self.class_name}",type_name="{self.type_name}",namespace="{self.namespace}",name="{self.name}")'


# <broker_type>://<broker_name>/<class>/<namespace>/<name>::<type> (juiz_sdk の OBJECT_IDENTIFIER_REGEX と同じ)。
# ネームスペースは / 区切りの階層で、空ならルート (<class>/<name>)。
# ContainerProcess の型名は "型:所属 Container 名" なので、型名には : が入る。
_IDENTIFIER_REGEX = re.compile(r'^(.+?)://(.+?)/(.+?)/(.+?)::(.+?)$')


def split_namespace(path: str):
    """'robot1/arm/inc0' を ('robot1/arm', 'inc0') に分ける。/ が無ければネームスペースは空 (ルート)。"""
    namespace, sep, name = path.rpartition('/')
    return (namespace, name) if sep else ('', path)


def join_namespace(namespace: str, name: str):
    """ネームスペースと名前をつなぐ。ネームスペースが空なら名前だけ。"""
    return f'{namespace}/{name}' if namespace else name


def is_in_namespace(namespace: str, filter: str):
    """namespace が filter と同じか、その下 (<filter>/...) にあれば True。filter が空か '/' ならすべて True。"""
    filter = filter[1:] if filter.startswith('/') else filter
    return (not filter
            or namespace == filter
            or namespace.startswith(filter + '/'))


def to_identifier(value, class_name=None):
    """list API の各要素を識別子の文字列にする。

    /api/process/list などは各要素を
    {"broker_type_name": ..., "broker_name": ..., "class_name": ..., "namespace": ..., "name": ..., "type_name": ...}
    のオブジェクトで返すので、broker_type_name://broker_name/class_name/namespace/name::type_name に組み立てる
    (namespace が空ならルートで、/namespace は付かない)。文字列ならそのまま返す。
    """
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        if isinstance(value.get('identifier'), str):
            return value['identifier']
        broker_type = value.get('broker_type_name') or 'core'
        broker_name = value.get('broker_name') or 'core'
        cn = value.get('class_name') or class_name
        full_name = join_namespace(value.get('namespace') or '', value['name'])
        return f"{broker_type}://{broker_name}/{cn}/{full_name}::{value['type_name']}"
    raise TypeError(f'invalid identifier: {value!r}')


def _parse(id):
    return IdentifierStruct.fromIdentifier(to_identifier(id))


def id_to_name(id: str):
    """ID からネームスペースを含まない短い名前を取り出す"""
    return _parse(id).name


def id_to_namespace(id: str):
    """ID からネームスペースを取り出す (ルートなら空文字列)"""
    return _parse(id).namespace


def id_to_full_name(id: str):
    """ID からネームスペースを含めた名前 (<namespace>/<name>) を取り出す"""
    return _parse(id).full_name()


def id_to_type_name(id: str):
    return _parse(id).type_name

if __name__ == '__main__':
    i = 'core://core/process/robot1/hoge_process_name0::hoge_process'
    print(id_to_namespace(i), id_to_name(i))
