
import argparse
import juiz
import json

def _get_system(option):
    return juiz.proxy.SystemProxy(juiz.proxy.HttpProxy(option.host, option.port))

def sys_handler(args):
    print('system:', args)
    
    
    
def proc_list_handler(option):
    s = _get_system(option)
    ps = s.processes()
    print([p.identifier() for p in ps])
    
def proc_call_handler(option):
    s = _get_system(option)
    p = s.process(option.identifier)
    body = json.loads(option.body)
    print(p.call(body))
    
def proc_exec_handler(option):
    s = _get_system(option)
    p = s.process(option.identifier)
    print(p.execute())
    
def proc_prof_handler(option):
    s = _get_system(option)
    p = s.process(option.identifier)
    print(p.profile_full())
    
def setup_process_parser(proc_parser):
    proc_subparsers = proc_parser.add_subparsers()
    p_l_parser = proc_subparsers.add_parser('list')
    p_l_parser.set_defaults(handler=proc_list_handler)
    
    p_p_parser = proc_subparsers.add_parser('profile')
    p_p_parser.add_argument('identifier', help='process ID')
    p_p_parser.set_defaults(handler=proc_prof_handler)
    
    p_c_parser = proc_subparsers.add_parser('call')
    p_c_parser.add_argument('identifier', help='process ID')
    p_c_parser.add_argument('body', help='body json')
    p_c_parser.set_defaults(handler=proc_call_handler)

    
def cont_list_handler(option):
    s = _get_system(option)
    cs = s.containers()
    print([c.identifier() for c in cs])
    
def cont_prof_handler(option):
    s = _get_system(option)
    c = s.container(option.identifier)
    print(c.profile_full())
    
def setup_container_parser(cont_parser):
    cont_subparsers = cont_parser.add_subparsers()
    c_l_parser = cont_subparsers.add_parser('list')
    c_l_parser.set_defaults(handler=cont_list_handler)
    
    c_p_parser =  cont_subparsers.add_parser('profile')
    c_p_parser.add_argument('identifier', help='container ID')
    c_p_parser.set_defaults(handler=cont_prof_handler)
    
    setup_container_process_parser(cont_subparsers.add_parser('process'))
        
        
    
def c_p_l_handler(option):
    s = _get_system(option)
    c = s.container(option.identifier)
    ps = c.processes()
    print([p.identifier() for p in ps])
    
    
def c_p_c_handler(option):
    s = _get_system(option)
    c = s.container(option.identifier)
    p = c.process(option.process_identifier)
    body = json.loads(option.body)
    print(p.call(body))
    
def c_p_e_handler(option):
    s = _get_system(option)
    c = s.container(option.identifier)
    p = c.process(option.process_identifier)
    print(p.execute())
    
def setup_container_process_parser(c_ps_parser):
    c_ps_parser.add_argument('identifier', help='container ID')
    c_ps_subparsers = c_ps_parser.add_subparsers()
    
    c_ps_list_parser = c_ps_subparsers.add_parser('list')
    c_ps_list_parser.set_defaults(handler=c_p_l_handler)

    c_ps_call_parser = c_ps_subparsers.add_parser('call')
    c_ps_call_parser.add_argument('process_identifier', help='container process id')
    c_ps_call_parser.add_argument('body', help='data')
    c_ps_call_parser.set_defaults(handler=c_p_c_handler)

    c_ps_e_parser = c_ps_subparsers.add_parser('execute')
    c_ps_e_parser.add_argument('process_identifier', help='container process id')
    c_ps_e_parser.set_defaults(handler=c_p_e_handler)
    
def main():
    parser = argparse.ArgumentParser(
        prog='juiz',
        description='juiz test program',
    )
    parser.add_argument('--host', default='localhost')
    parser.add_argument('--port', default=8000)
    subparsers = parser.add_subparsers()
    sysparser = subparsers.add_parser('system', help='system command')
    sysparser.add_argument('profile', help='system profile')
    sysparser.set_defaults(handler=sys_handler)
    
    setup_process_parser(subparsers.add_parser('process', help='process command'))
    setup_container_parser(subparsers.add_parser('container', help='container command'))
    
    
    option = parser.parse_args()
        
    if hasattr(option, 'handler'):
        option.handler(option)

if __name__ == '__main__':
    main()