const juiz = {
  ContainerProcess : class {
    constructor(owner, id) {
        this.owner = id;
        this.id = id;
    }

    type_name() {
        return this.profile.type_name;
    }

    async setup() {
        console.log('ContainerProcess.setup(', this.id, ')');
        this.profile = await this.profile_full();
        console.log(' - profile:', this.profile);
        return this.profile;
    }

    async profile_full() {
        const url = "/api/container_process/profile?identifier=" + this.id;
        const response = await fetch(url, {
            "Content-Type": "application/json"
        });
        return await response.json();
    }

    async call_process(args) {
        const url = "/api/container_process/call?identifier=" + this.id;
        let arg_data = {};
        if (args !== undefined) {
            arg_data = args;
        }
        const response = await fetch(url, {
            method: "PATCH",
            headers: {
                accept: "*/*",
                "Content-Type": "application/json",
            },
            body: JSON.stringify(arg_data),
        });
        if (response.status === 200) {
            const contentType = response.headers.get('Content-Type');
            if (contentType === 'application/json') {
                return await response.json();
            } else {
                return await response.blob();
            }
        }
        return null;
    }
  }, 
  Container : class {
    constructor(id) {
        this.id = id;
    }

    type_name() {
        return this.profile.type_name;
    }

    async setup() {
        console.log('Container.setup(', this.id, ')');
        this._processes = null;
        this.profile = await this.profile_full();
        console.log(' - profile:', this.profile);
        return this.profile;
    }

    async processes() {
        if (this._processes === null) {
            let ps = [];
            for (let proc_id of this.profile.processes) {
                let p = new juiz.Process(this, proc_id);
                await p.setup();
                ps.push(p);
            }
            this._processes = ps;
        }
        return this._processes;
    }

    async process(query) {
        if (query.type_name !== undefined) {
            for (let p of await this.processes()) {
                if (p.type_name() === query.type_name) {
                    return p;
                }
            }
        }
        return null;
    }

    async profile_full() {
        const url = "/api/container/profile?identifier=" + this.id;
        const response = await fetch(url, {
            "Content-Type": "application/json"
        });
        return await response.json();
    }
  },
  Process : class {
    constructor(id) {
        this.id = id;
    }

    type_name() {
        return this.profile.type_name;
    }

    async setup() {
        console.log('Process.setup(', this.id, ')');
        this.profile = await this.profile_full();
        console.log(' - profile:', this.profile);
        return this.profile;
    }

    async profile_full() {
        const url = "/api/process/profile?identifier=" + this.id;
        const response = await fetch(url, {
            "Content-Type": "application/json"
        });
        return await response.json();
    }

    async call_process(args) {
        const url = "/api/process/call?identifier=" + this.id;
        let arg_data = {};
        if (args !== undefined) {
            arg_data = args;
        }
        const response = await fetch(url, {
            method: "PATCH",
            headers: {
                accept: "*/*",
                "Content-Type": "application/json",
            },
            body: JSON.stringify(arg_data),
        });
        if (response.status === 200) {
            const contentType = response.headers.get('Content-Type');
            if (contentType === 'application/json') {
                return await response.json();
            } else {
                return await response.blob();
            }
        }
        return null;
    }

  },

  System : class {
    constructor() {
        this._containers = null;
        this._processes = null;
    }


    async setup() {
        this.profile = await this.profile_full();
        return this.profile;
    }

    async containers() {
        if (this._containers === null) {
            let cs = [];
            for (let container_id in this.profile.core_store.containers) {
                let c = new juiz.Container(container_id);
                await c.setup();
                cs.push(c);
            }
            this._containers = cs;
        }
        return this._containers.map((c) => {return c;})
    }

    async processes() {
        if (this._processes === null) {
            let ps = [];
            for (let process_id in this.profile.core_store.processes) {
                let p = new juiz.Process(process_id);
                await p.setup();
                ps.push(p);
            }
            this._processes = ps;
        }
        return this._processes.map((p) => {return p;})
    }

    async container(query) {
        const containers = await this.containers();
        if (query.type_name !== undefined) {
            for (let c of containers) {
                if (c.type_name() === query.type_name) {
                    return c;
                }
            }
        }
        return null;
    }

    async process(query) {
        const processes = await this.processes();
        if (query.type_name !== undefined) {
            for (let p of processes) {
                if (p.type_name() === query.type_name) {
                    return p;
                }
            }
        }
        return null;
    }

    async profile_full() {
        const url = "/api/system/profile";
        const response = await fetch(url, {
            "Content-Type": "application/json"
        });
        const prof = await response.json();
        return prof;
    }
  }
};