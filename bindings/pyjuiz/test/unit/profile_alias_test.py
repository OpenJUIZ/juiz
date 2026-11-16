"""pyjuiz の profile() と古い名前 profile_full() のテスト (issue #508、docs/spec/manifest-profile.md §7)

サーバーは使わない。bindings/pyjuiz で実行する:

    python test/unit/profile_alias_test.py
"""
import sys
import warnings

sys.path.append('.')

from juiz.proxy import CRUDProxy, SystemProxy, ProcessProxy, NotFoundError, InternalServerError


class FakeCrud(CRUDProxy):
    """呼ばれた (class_name, function_name) を覚える CRUDProxy。

    new=False なら古い juiz のふりをして、profile には古い juiz と同じ 500 とエラーの文を返す"""

    def __init__(self, new=True, not_found=NotFoundError):
        self.new = new
        self.not_found = not_found
        self.calls = []

    def read(self, class_name, function_name, param_dict={}):
        self.calls.append((class_name, function_name))
        if function_name == 'profile' and not self.new:
            raise self.not_found(
                f'Internal Server Error:  CRUDBroker can not detect function (profile) in class_name ({class_name})')
        if function_name in ('profile', 'profile_full'):
            return {'name': 'p', 'class_name': class_name, 'via': function_name}
        raise InternalServerError(f'unknown {function_name}')


def check_new_juiz():
    crud = FakeCrud(new=True)
    s = SystemProxy(crud)
    assert s.profile()['via'] == 'profile'
    assert crud.calls == [('system', 'profile')]


def check_old_juiz_falls_back():
    crud = FakeCrud(new=False, not_found=InternalServerError)
    s = SystemProxy(crud)
    assert s.profile()['via'] == 'profile_full'
    # 古いと分かった相手には、はじめから profile_full
    assert s.composite('t')['via'] == 'profile_full'
    assert crud.calls == [('system', 'profile'), ('system', 'profile_full'), ('composite', 'profile_full')]


def check_404_falls_back():
    crud = FakeCrud(new=False, not_found=NotFoundError)
    assert SystemProxy(crud).profile()['via'] == 'profile_full'


def check_other_errors_are_raised():
    class Broken(FakeCrud):
        def read(self, class_name, function_name, param_dict={}):
            self.calls.append((class_name, function_name))
            raise InternalServerError('process not found')
    crud = Broken()
    try:
        SystemProxy(crud).profile()
        assert False, 'must raise'
    except InternalServerError:
        pass
    assert crud.calls == [('system', 'profile')]


def check_profile_full_is_deprecated():
    crud = FakeCrud(new=True)
    s = SystemProxy(crud)
    p = ProcessProxy.new(s, 'core://core/process/p0::t')
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter('always')
        assert s.profile_full()['via'] == 'profile'
        assert p.profile_full()['via'] == 'profile'
        assert p.profile_full(update=True)['via'] == 'profile'
    deprecations = [x for x in w if issubclass(x.category, DeprecationWarning)]
    # 名前ごとに初めの 1 回だけ
    assert len(deprecations) == 2, [str(x.message) for x in deprecations]


def main():
    check_new_juiz()
    check_old_juiz_falls_back()
    check_404_falls_back()
    check_other_errors_are_raised()
    check_profile_full_is_deprecated()
    print('ok')


if __name__ == '__main__':
    main()
