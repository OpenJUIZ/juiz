// node scripts/gen-api.mjs で生成。直接編集しない。

export interface paths {
    "/api/anchor/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Anchor の ID の配列 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/anchor/update": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {} */
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description {identifier, stamp} */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/any_process/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                    namespace?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/any_process/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/broker/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/broker/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/composite/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"__manifest__": <Composite の定義 (conf の composites: の 1 件と同じ形)>} */
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description Factory の manifest */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/composite/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 消した Factory の manifest */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/composite/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Composite の型名の配列 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/composite/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Factory の manifest */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/composite_container/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"__manifest__": <conf の composite_containers: の 1 件と同じ形>} */
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description CompositeContainer の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/connection/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["CreateConnectionRequest"];
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/connection/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/connection/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                    namespace?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/pose": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    /** @description Container の ID か名前 (根のネームスペースから解決する) */
                    identifier?: string | null;
                    /** @description 基準 Container の ID か名前 */
                    reference?: string | null;
                    /** @description 基準を、この UUID の System から取り込んだ写しだけから探す (別 System から回されたとき、§19.8) */
                    reference_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 基準から見た ContainerPose (juiz_std.geometry.PoseStamped) */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["juiz_std.geometry.PoseStamped"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/pose_state": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    /** @description Container の ID か名前 */
                    identifier?: string | null;
                    /** @description 前回受け取った version。同じなら entries を省く */
                    since?: string | null;
                    /** @description 前回受け取った system (相手の UUID)。今の UUID と違えば since を無視する */
                    system?: string | null;
                    /** @description 取りに来た System の UUID (container/profile_full の remote_children に出る) */
                    subscriber?: string | null;
                    /** @description 取りに来た System での終点の ID (複数なら `,` 区切り) */
                    destination?: string | null;
                    /** @description 取りに来た側の timeout [秒] (remote_children から消すまでの時間、既定 3.0) */
                    timeout?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description {identifier, system, version, changed, entries: [{anchor, origin_system, origin_anchor, pose, stamp, version}]} (System をまたぐ Transform の取り込みに使う、§19.4) */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Container */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container/set_pose": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    /** @description Container の ID か名前 */
                    identifier?: string | null;
                    /** @description 基準を、この UUID の System から取り込んだ写しだけから探す (§19.8) */
                    reference_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"reference": <基準 Container>, "pose": <juiz_std.geometry.Pose>} */
            requestBody: {
                content: {
                    "application/json": {
                        pose: components["schemas"]["juiz_std.geometry.Pose"];
                        /** @description 基準 Container の ID か名前 */
                        reference: string;
                    };
                };
            };
            responses: {
                /** @description 書いた後の ContainerPose (juiz_std.geometry.PoseStamped) */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["juiz_std.geometry.PoseStamped"];
                    };
                };
            };
        };
        trace?: never;
    };
    "/api/container_process/call": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/container_process/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container_process/execute": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/container_process/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                    namespace?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container_process/openapi_spec": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/container_process/p_apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/container_process/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/execution_context/get_state": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/execution_context/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                    namespace?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/execution_context/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/execution_context/start": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/execution_context/stop": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/pose_listener/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"container", "process", "arg"?, "reference"?, "on_update"?, "initial"?} */
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description Pose 通知の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/pose_listener/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 外した Pose 通知の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/pose_listener/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Pose 通知の ID の配列 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/pose_listener/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description {identifier, container, process, arg, reference, on_update, initial} */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/process/call": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/process/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/process/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/process/execute": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/process/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                    namespace?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/process/openapi_spec": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/process/p_apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/process/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/add_mastersystem": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                    path?: string | null;
                    recursive?: string | null;
                    system_uuid?: string | null;
                    topic_name?: string | null;
                    namespace?: string | null;
                    /** @description Transformations の基準 Container (`container/pose`、issue #370) */
                    reference?: string | null;
                    /** @description Transformations の Anchor (`transformations/tree`、issue #370) */
                    anchor?: string | null;
                    /** @description `container/pose_state` の前回の版番号・相手の UUID (transformations.md §19.4) */
                    since?: string | null;
                    system?: string | null;
                    /** @description `container/pose_state` を取りに来た System の UUID・終点 (`,` 区切り)・timeout (§19.10) */
                    subscriber?: string | null;
                    destination?: string | null;
                    timeout?: string | null;
                    /** @description 基準を写しから探す System の UUID (`container/pose`・`set_pose`、§19.8) */
                    reference_system?: string | null;
                    /** @description 回された Transform の始点の System の UUID (`transform/*`、§19.2 の 7) */
                    source_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/system/add_subsystem": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/system/filesystem_list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    path?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System. recursive=true ならサブシステムの中身と各 System の接続 (connections) も入れる */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/system/remove_subsystem": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"uuid": 外すサブシステムの UUID} */
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description 外したサブシステムの uuid と broker_proxy */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/system/uuid": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/topic/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    recursive?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/topic/push": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    topic_name?: string | null;
                    system_uuid?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/topic/request_subscribe": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    topic_name?: string | null;
                    system_uuid?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": unknown;
                };
            };
            responses: {
                /** @description System */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        trace?: never;
    };
    "/api/transform/create": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"source": <始点>, "destination": <終点>, "translation": <juiz_std.geometry.Vector3>, "rotation": <juiz_std.geometry.Quaternion>, "rate": <Hz>, "timeout": <秒>}。translation・rotation・rate・timeout は省略可。終点が別 System なら、その System で作る */
            requestBody: {
                content: {
                    "application/json": {
                        /** @description 終点 (子) の Container の ID か名前 */
                        destination: string;
                        /** @description System をまたぐ Transform の取り込みの周期 [Hz] (既定 10.0) */
                        rate?: number;
                        rotation?: components["schemas"]["juiz_std.geometry.Quaternion"];
                        /** @description 始点 (親) の Container の ID か名前 */
                        source: string;
                        /** @description 別 System から回されたとき、始点の System の UUID */
                        source_system?: string;
                        /** @description 取り込めないとき写しを消すまでの時間 [秒] (既定 3.0) */
                        timeout?: number;
                        translation?: components["schemas"]["juiz_std.geometry.Vector3"];
                    };
                };
            };
            responses: {
                /** @description Transform の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            destination?: string;
                            identifier?: string;
                            /** @description System をまたぐ Transform の取り込みの様子 (§19.10) */
                            remote?: {
                                last_success?: components["schemas"]["juiz_std.common.Time"];
                                rate?: number;
                                /** @enum {string} */
                                state?: "ok" | "stale" | "lost";
                                system?: string | null;
                                timeout?: number;
                            };
                            source?: string;
                            stamp?: components["schemas"]["juiz_std.common.Time"];
                            transform?: components["schemas"]["juiz_std.geometry.Transform"];
                        };
                    };
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/transform/destroy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: {
            parameters: {
                query?: {
                    /** @description Transform の ID (transform://<始点>|<終点>) */
                    identifier?: string | null;
                    /** @description 別 System から回されたとき、始点の System の UUID (§19.2 の 7) */
                    source_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 消した Transform の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            destination?: string;
                            identifier?: string;
                            /** @description System をまたぐ Transform の取り込みの様子 (§19.10) */
                            remote?: {
                                last_success?: components["schemas"]["juiz_std.common.Time"];
                                rate?: number;
                                /** @enum {string} */
                                state?: "ok" | "stale" | "lost";
                                system?: string | null;
                                timeout?: number;
                            };
                            source?: string;
                            stamp?: components["schemas"]["juiz_std.common.Time"];
                            transform?: components["schemas"]["juiz_std.geometry.Transform"];
                        };
                    };
                };
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/transform/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description Transform の ID (transform://<始点>|<終点>) の配列 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/transform/profile_full": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    /** @description Transform の ID (transform://<始点>|<終点>) */
                    identifier?: string | null;
                    /** @description 別 System から回されたとき、始点の System の UUID (§19.2 の 7) */
                    source_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description {identifier, source, destination, transform, stamp, remote?} */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            destination?: string;
                            identifier?: string;
                            /** @description System をまたぐ Transform の取り込みの様子 (§19.10) */
                            remote?: {
                                last_success?: components["schemas"]["juiz_std.common.Time"];
                                rate?: number;
                                /** @enum {string} */
                                state?: "ok" | "stale" | "lost";
                                system?: string | null;
                                timeout?: number;
                            };
                            source?: string;
                            stamp?: components["schemas"]["juiz_std.common.Time"];
                            transform?: components["schemas"]["juiz_std.geometry.Transform"];
                        };
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/transform/update": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: {
            parameters: {
                query?: {
                    /** @description Transform の ID (transform://<始点>|<終点>) */
                    identifier?: string | null;
                    /** @description 別 System から回されたとき、始点の System の UUID (§19.2 の 7) */
                    source_system?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description {"translation": <Vector3>, "rotation": <Quaternion>}。省いたものは今のまま */
            requestBody: {
                content: {
                    "application/json": {
                        rotation?: components["schemas"]["juiz_std.geometry.Quaternion"];
                        translation?: components["schemas"]["juiz_std.geometry.Vector3"];
                    };
                };
            };
            responses: {
                /** @description Transform の profile */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            destination?: string;
                            identifier?: string;
                            /** @description System をまたぐ Transform の取り込みの様子 (§19.10) */
                            remote?: {
                                last_success?: components["schemas"]["juiz_std.common.Time"];
                                rate?: number;
                                /** @enum {string} */
                                state?: "ok" | "stale" | "lost";
                                system?: string | null;
                                timeout?: number;
                            };
                            source?: string;
                            stamp?: components["schemas"]["juiz_std.common.Time"];
                            transform?: components["schemas"]["juiz_std.geometry.Transform"];
                        };
                    };
                };
            };
        };
        trace?: never;
    };
    "/api/transformations/tree": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    /** @description Anchor の ID か名前 */
                    anchor?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description {identifier, pose, transform?, children: [...]} の入れ子 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/type/list": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 登録している名前付き型の配列 ({identifier, name, fingerprint, short_identifier, kind, used_by}) */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/type/schema": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: {
            parameters: {
                query?: {
                    identifier?: string | null;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 型参照 {name, fingerprint, avro}。identifier は <完全名>@sha256:… か完全名だけ */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        CreateConnectionRequest: {
            arg_name: string;
            destination: components["schemas"]["IdentifierManifest"];
            source: components["schemas"]["IdentifierManifest"];
        };
        IdentifierManifest: {
            identifier: string;
        };
        /**
         * juiz_std.common.Header
         * @description データの時刻と座標系
         */
        "juiz_std.common.Header": {
            /**
             * @description 座標系の名前
             * @default
             */
            frame_id: string;
            /**
             * @description データを取った時刻
             * @default {
             *       "nanosec": 0,
             *       "sec": 0
             *     }
             */
            stamp: components["schemas"]["juiz_std.common.Time"];
        };
        /**
         * juiz_std.common.Time
         * @description 時刻 (1970-01-01T00:00:00Z からの経過時間)
         */
        "juiz_std.common.Time": {
            /**
             * Format: int32
             * @description 秒未満 [ns] (0〜999999999)
             * @default 0
             */
            nanosec: number;
            /**
             * Format: int64
             * @description 秒
             * @default 0
             */
            sec: number;
        };
        /**
         * juiz_std.geometry.Point3D
         * @description 空間の点
         */
        "juiz_std.geometry.Point3D": {
            /**
             * Format: double
             * @default 0
             */
            x: number;
            /**
             * Format: double
             * @default 0
             */
            y: number;
            /**
             * Format: double
             * @default 0
             */
            z: number;
        };
        /**
         * juiz_std.geometry.Pose
         * @description 空間の姿勢
         */
        "juiz_std.geometry.Pose": {
            /**
             * @default {
             *       "w": 1,
             *       "x": 0,
             *       "y": 0,
             *       "z": 0
             *     }
             */
            orientation: components["schemas"]["juiz_std.geometry.Quaternion"];
            /**
             * @default {
             *       "x": 0,
             *       "y": 0,
             *       "z": 0
             *     }
             */
            position: components["schemas"]["juiz_std.geometry.Point3D"];
        };
        /**
         * juiz_std.geometry.PoseStamped
         * @description 時刻と座標系つきの空間の姿勢
         */
        "juiz_std.geometry.PoseStamped": {
            /**
             * @default {
             *       "frame_id": "",
             *       "stamp": {
             *         "nanosec": 0,
             *         "sec": 0
             *       }
             *     }
             */
            header: components["schemas"]["juiz_std.common.Header"];
            /**
             * @default {
             *       "orientation": {
             *         "w": 1,
             *         "x": 0,
             *         "y": 0,
             *         "z": 0
             *       },
             *       "position": {
             *         "x": 0,
             *         "y": 0,
             *         "z": 0
             *       }
             *     }
             */
            pose: components["schemas"]["juiz_std.geometry.Pose"];
        };
        /**
         * juiz_std.geometry.Quaternion
         * @description 回転を表す四元数 (既定値は回転なし)
         */
        "juiz_std.geometry.Quaternion": {
            /**
             * Format: double
             * @default 1
             */
            w: number;
            /**
             * Format: double
             * @default 0
             */
            x: number;
            /**
             * Format: double
             * @default 0
             */
            y: number;
            /**
             * Format: double
             * @default 0
             */
            z: number;
        };
        /**
         * juiz_std.geometry.Transform
         * @description 座標変換 (ROS 2 の geometry_msgs/Transform と同じ形)。
         *     子の座標を親の座標へ写す: p_親 = rotation · p_子 + translation (docs/spec/transformations.md)
         */
        "juiz_std.geometry.Transform": {
            /**
             * @default {
             *       "w": 1,
             *       "x": 0,
             *       "y": 0,
             *       "z": 0
             *     }
             */
            rotation: components["schemas"]["juiz_std.geometry.Quaternion"];
            /**
             * @default {
             *       "x": 0,
             *       "y": 0,
             *       "z": 0
             *     }
             */
            translation: components["schemas"]["juiz_std.geometry.Vector3"];
        };
        /**
         * juiz_std.geometry.TransformStamped
         * @description 時刻と座標系つきの座標変換 (ROS 2 の geometry_msgs/TransformStamped と同じ形)。
         *     header.frame_id が親 (始点)、child_frame_id が子 (終点)
         */
        "juiz_std.geometry.TransformStamped": {
            /** @default  */
            child_frame_id: string;
            /**
             * @default {
             *       "frame_id": "",
             *       "stamp": {
             *         "nanosec": 0,
             *         "sec": 0
             *       }
             *     }
             */
            header: components["schemas"]["juiz_std.common.Header"];
            /**
             * @default {
             *       "rotation": {
             *         "w": 1,
             *         "x": 0,
             *         "y": 0,
             *         "z": 0
             *       },
             *       "translation": {
             *         "x": 0,
             *         "y": 0,
             *         "z": 0
             *       }
             *     }
             */
            transform: components["schemas"]["juiz_std.geometry.Transform"];
        };
        /**
         * juiz_std.geometry.Vector3
         * @description 空間のベクトル
         */
        "juiz_std.geometry.Vector3": {
            /**
             * Format: double
             * @default 0
             */
            x: number;
            /**
             * Format: double
             * @default 0
             */
            y: number;
            /**
             * Format: double
             * @default 0
             */
            z: number;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export type operations = Record<string, never>;
