export interface WebClientConfig {
    config_server_url?: string
}

export interface NormalMode extends WebClientConfig {
    mode: 'normal'
    // if not provided will use ring tunnel rpc server
    rpc_portal?: string
    enable_rpc_port_listen?: boolean
    rpc_listen_port?: number
}

export function saveMode(mode: Mode) {
    localStorage.setItem('app_mode', JSON.stringify(mode))
}


export function loadMode(): Mode {
    // ANFAGENT-30：仅保留客户端（normal）模式，忽略历史保存的 service/remote 模式。
    try {
        const modeStr = localStorage.getItem('app_mode')
        if (modeStr) {
            const parsed = JSON.parse(modeStr) as Partial<NormalMode>
            if (parsed && parsed.mode === 'normal') {
                const { config_server_url, rpc_portal, enable_rpc_port_listen, rpc_listen_port } = parsed
                return {
                    mode: 'normal',
                    config_server_url,
                    rpc_portal,
                    enable_rpc_port_listen,
                    rpc_listen_port,
                }
            }
        }
    } catch {
        // 损坏的本地值直接走默认
    }
    return { mode: 'normal' }
}

export type Mode = NormalMode
