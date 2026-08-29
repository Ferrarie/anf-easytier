import axios, { AxiosError, AxiosInstance, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import { type Api, NetworkTypes, Utils } from 'easytier-frontend-lib';
import { Md5 } from 'ts-md5';

export interface ValidateConfigResponse {
    toml_config: string;
}

export interface OidcConfigResponse {
    enabled: boolean;
}

// 定义接口返回的数据结构
export interface LoginResponse {
    success: boolean;
    message: string;
    /** 登录需要两步验证（半会话已建立，跳动态码页） */
    require_2fa?: boolean;
    /** superuser 未绑定 2FA，需先完成绑定（跳绑定页而非验码页） */
    setup_required?: boolean;
}

export interface RegisterResponse {
    success: boolean;
    message: string;
}

// 定义请求体数据结构
export interface Credential {
    username: string;
    password: string;
}

export interface RegisterData {
    credentials: Credential;
    captcha: string;
}

export interface Summary {
    device_count: number;
}

export interface CenterInfo {
    version: string;
    api_server_port: number;
    web_server_port?: number;
    config_server_protocol: string;
    config_server_port: number;
    anf_network_name: string;
    anf_center_peer_url?: string;
}

export interface AnfStats {
    total_devices: number;
    pending: number;
    approved: number;
    rejected: number;
    kicked: number;
    networks: number;
    tags: number;
    rules: number;
}

export interface ListNetworkInstanceIdResponse {
    running_inst_ids: Array<Utils.UUID>,
    disabled_inst_ids: Array<Utils.UUID>,
}

export interface GenerateConfigRequest {
    config: NetworkTypes.NetworkConfig;
}

export interface GenerateConfigResponse {
    toml_config?: string;
    error?: string;
}

export interface ParseConfigRequest {
    toml_config: string;
}

export interface ParseConfigResponse {
    config?: NetworkTypes.NetworkConfig;
    error?: string;
}

export class ApiClient {
    private client: AxiosInstance;
    private authFailedCb: Function | undefined;

    constructor(baseUrl: string, authFailedCb: Function | undefined = undefined) {
        this.client = axios.create({
            baseURL: baseUrl.replace(/\/+$/, '') + '/api/v1',
            withCredentials: true, // 如果需要支持跨域携带cookie
            headers: {
                'Content-Type': 'application/json',
            },
        });
        this.authFailedCb = authFailedCb;

        // 添加请求拦截器
        this.client.interceptors.request.use((config: InternalAxiosRequestConfig) => {
            return config;
        }, (error: any) => {
            return Promise.reject(error);
        });

        // 添加响应拦截器
        this.client.interceptors.response.use((response: AxiosResponse) => {
            return response.data; // 假设服务器返回的数据都在data属性中
        }, (error: any) => {
            if (error.response) {
                let response: AxiosResponse = error.response;
                if (response.status == 401 && this.authFailedCb) {
                    console.error('Unauthorized:', response.data);
                    this.authFailedCb();
                } else {
                    // 请求已发出，但是服务器响应的状态码不在2xx范围
                    console.error('Response Error:', error.response.data);
                }
            } else if (error.request) {
                // 请求已发出，但是没有收到响应
                console.error('Request Error:', error.request);
            } else {
                // 发生了一些问题导致请求未发出
                console.error('Error:', error.message);
            }
            return Promise.reject(error);
        });
    }

    // 注册
    public async register(data: RegisterData): Promise<RegisterResponse> {
        try {
            data.credentials.password = Md5.hashStr(data.credentials.password);
            await this.client.post<RegisterResponse>('/auth/register', data);
            return { success: true, message: 'Register success', };
        } catch (error) {
            if (error instanceof AxiosError) {
                return { success: false, message: 'Failed to register, error: ' + JSON.stringify(error.response?.data), };
            }
            return { success: false, message: 'Unknown error, error: ' + error, };
        }
    }

    // 登录
    public async login(data: Credential): Promise<LoginResponse> {
        try {
            data.password = Md5.hashStr(data.password);
            const ret = await this.client.post<any, any>('/auth/login', data);
            return { success: true, message: 'Login success', require_2fa: !!ret?.require_2fa, setup_required: !!ret?.setup_required, };
        } catch (error) {
            if (error instanceof AxiosError) {
                if (error.response?.status === 401) {
                    return { success: false, message: 'Invalid username or password', };
                } else {
                    return { success: false, message: 'Unknown error, status code: ' + error.response?.status, };
                }
            }
            return { success: false, message: 'Unknown error, error: ' + error, };
        }
    }

    public async logout() {
        await this.client.get('/auth/logout');
        if (this.authFailedCb) {
            this.authFailedCb();
        }
    }

    public async change_password(new_password: string) {
        await this.client.put('/auth/password', { new_password: Md5.hashStr(new_password) });
    }

    public async check_login_status() {
        try {
            await this.client.get('/auth/check_login_status');
            return true;
        } catch (error) {
            return false;
        }
    }

    /** 登录态 + superuser 是否需强制绑定 2FA（路由守卫用） */
    public async check_login_status_detail(): Promise<{ logged_in: boolean; require_two_factor_setup: boolean }> {
        try {
            const ret = await this.client.get<any, any>('/auth/check_login_status');
            return { logged_in: true, require_two_factor_setup: !!ret?.require_two_factor_setup };
        } catch (error) {
            return { logged_in: false, require_two_factor_setup: false };
        }
    }

    public async list_session() {
        const response = await this.client.get('/sessions');
        return response;
    }

    public async list_machines(): Promise<Array<any>> {
        const response = await this.client.get<any, Record<string, Array<any>>>('/machines');
        return response.machines;
    }

    // ===== ANFAGENT-30 M1：设备注册 / 审批 / 分配 =====

    /** 设备凭邀请码注册（公开） */
    public async registerDevice(inviteCode: string, machineId: string): Promise<any> {
        return this.client.post('/devices/register', {
            invite_code: inviteCode,
            machine_id: machineId,
        });
    }

    /** 设备列表（管理员），status 可选 pending/approved/rejected/kicked */
    public async listDevices(status?: string): Promise<Array<any>> {
        const params = status ? { status } : {};
        return this.client.get<any, Array<any>>('/devices', { params });
    }

    public async approveDevice(id: number): Promise<any> {
        return this.client.post(`/devices/${id}/approve`);
    }

    public async rejectDevice(id: number): Promise<any> {
        return this.client.post(`/devices/${id}/reject`);
    }

    public async kickDevice(id: number): Promise<any> {
        return this.client.post(`/devices/${id}/kick`);
    }

    public async updateDevice(id: number, payload: {
        display_name?: string;
        tags?: Array<string>;
        networks?: Array<string>;
    }): Promise<any> {
        return this.client.patch(`/devices/${id}`, payload);
    }

    /** 删除设备（tailscale 授权页的“移除机器”语义）。 */
    public async deleteDevice(id: number): Promise<any> {
        return this.client.delete(`/devices/${id}`);
    }

    // ===== ANFAGENT-30 M1：邀请码管理（管理员） =====

    public async listInvites(): Promise<Array<any>> {
        return this.client.get<any, Array<any>>('/invites');
    }

    public async createInvite(maxUses: number, expiresAt?: string): Promise<any> {
        return this.client.post('/invites', {
            max_uses: maxUses,
            expires_at: expiresAt || null,
        });
    }

    public async disableInvite(id: number): Promise<any> {
        return this.client.delete(`/invites/${id}`);
    }

    // ===== ANFAGENT-30 M2：网络 / tag / ACL =====

    public async listNetworks(): Promise<Array<any>> {
        return this.client.get<any, Array<any>>('/networks');
    }

    public async createNetwork(name: string, cidr?: string): Promise<any> {
        return this.client.post('/networks', { name, cidr: cidr || null });
    }

    public async deleteNetwork(id: string): Promise<any> {
        return this.client.delete(`/networks/${id}`);
    }

    public async networkDevices(id: string): Promise<Array<any>> {
        return this.client.get<any, Array<any>>(`/networks/${id}/devices`);
    }

    public async listTags(): Promise<Array<any>> {
        return this.client.get<any, Array<any>>('/tags');
    }

    public async createTag(name: string): Promise<any> {
        return this.client.post('/tags', { name });
    }

    public async deleteTag(id: number): Promise<any> {
        return this.client.delete(`/tags/${id}`);
    }

    public async updateTag(id: number, name: string): Promise<any> {
        return this.client.patch(`/tags/${id}`, { name });
    }

    public async listAclRules(networkId: string): Promise<Array<any>> {
        return this.client.get<any, Array<any>>(`/networks/${networkId}/rules`);
    }

    public async createAclRule(networkId: string, rule: any): Promise<any> {
        return this.client.post(`/networks/${networkId}/rules`, rule);
    }

    public async deleteAclRule(networkId: string, ruleId: number): Promise<any> {
        return this.client.delete(`/networks/${networkId}/rules/${ruleId}`);
    }

    public async updateAclRule(networkId: string, ruleId: number, rule: any): Promise<any> {
        return this.client.patch(`/networks/${networkId}/rules/${ruleId}`, rule);
    }

    // ===== ANF TOTP 两步验证（2FA，Gitea 同款两步式登录） =====

    /** 半会话状态探测（验码页刷新后恢复流程；pending=false 应回登录页） */
    public async get2faPending(): Promise<{ pending: boolean; setup_required?: boolean }> {
        return this.client.get('/auth/2fa/pending');
    }

    /** 校验动态码并建立正式会话；setup_required=true 时需强制引导绑定 */
    public async verify2fa(code: string): Promise<{ setup_required?: boolean }> {
        return this.client.post('/auth/2fa/verify', { code });
    }

    /** 当前登录用户的 2FA 状态 */
    public async get2faStatus(): Promise<{ enabled: boolean; is_superuser: boolean; setup_required: boolean }> {
        return this.client.get('/auth/2fa/status');
    }

    /** 生成 TOTP secret + otpauth URI（绑定第一步） */
    public async setup2fa(): Promise<{ secret: string; otpauth_url: string }> {
        return this.client.post('/auth/2fa/setup');
    }

    /** 输入动态码启用 2FA（绑定第二步） */
    public async enable2fa(code: string): Promise<any> {
        return this.client.post('/auth/2fa/enable', { code });
    }

    /** 输入当前动态码关闭 2FA */
    public async disable2fa(code: string): Promise<any> {
        return this.client.post('/auth/2fa/disable', { code });
    }

    /** 用户列表（管理员，含 2FA 状态） */
    public async adminListUsers(): Promise<Array<any>> {
        return this.client.get<any, Array<any>>('/admin/users');
    }

    /** 重置用户 2FA（管理员救援：验证器丢失） */
    public async adminReset2fa(userId: number): Promise<any> {
        return this.client.post(`/admin/users/${userId}/reset-2fa`);
    }

    public async get_summary(): Promise<Summary> {
        const response = await this.client.get<any, Summary>('/summary');
        return response;
    }

    public async centerInfo(): Promise<CenterInfo> {
        const response = await this.client.get<any, CenterInfo>('/center/info');
        return response;
    }

    public async anfStats(): Promise<AnfStats> {
        const response = await this.client.get<any, AnfStats>('/anf/stats');
        return response;
    }

    public captcha_url() {
        return this.client.defaults.baseURL + '/auth/captcha';
    }

    public async getOidcConfig(): Promise<OidcConfigResponse> {
        try {
            const response = await this.client.get<any, OidcConfigResponse>('/auth/oidc/config');
            return response;
        } catch (error) {
            return { enabled: false };
        }
    }

    public oidcLoginUrl() {
        return this.client.defaults.baseURL + '/auth/oidc/login';
    }

    public get_remote_client(machine_id: string): Api.RemoteClient {
        return new WebRemoteClient(machine_id, this.client);
    }
}

class WebRemoteClient implements Api.RemoteClient {
    private machine_id: string;
    private client: AxiosInstance;

    constructor(machine_id: string, client: AxiosInstance) {
        this.machine_id = machine_id;
        this.client = client;
    }
    async validate_config(config: NetworkTypes.NetworkConfig): Promise<Api.ValidateConfigResponse> {
        const response = await this.client.post<NetworkTypes.NetworkConfig, ValidateConfigResponse>(`/machines/${this.machine_id}/validate-config`, {
            config: NetworkTypes.toBackendNetworkConfig(config),
        });
        return response;
    }
    async run_network(config: NetworkTypes.NetworkConfig, save: boolean): Promise<undefined> {
        await this.client.post<string>(`/machines/${this.machine_id}/networks`, {
            config: NetworkTypes.toBackendNetworkConfig(config),
            save: save
        });
    }
    async get_network_info(inst_id: string): Promise<NetworkTypes.NetworkInstanceRunningInfo | undefined> {
        const response = await this.client.get<any, Api.CollectNetworkInfoResponse>('/machines/' + this.machine_id + '/networks/info/' + inst_id);
        return response.info?.map?.[inst_id];
    }
    async list_network_instance_ids(): Promise<Api.ListNetworkInstanceIdResponse> {
        const response = await this.client.get<any, ListNetworkInstanceIdResponse>('/machines/' + this.machine_id + '/networks');
        return response;
    }
    async delete_network(inst_id: string): Promise<undefined> {
        await this.client.delete<string>(`/machines/${this.machine_id}/networks/${inst_id}`);
    }
    async update_network_instance_state(inst_id: string, disabled: boolean): Promise<undefined> {
        await this.client.put<string>('/machines/' + this.machine_id + '/networks/' + inst_id, {
            disabled: disabled,
        });
    }
    async save_config(config: NetworkTypes.NetworkConfig): Promise<undefined> {
        await this.client.put(`/machines/${this.machine_id}/networks/config/${config.instance_id}`, {
            config: NetworkTypes.toBackendNetworkConfig(config)
        });
    }
    async get_network_config(inst_id: string): Promise<NetworkTypes.NetworkConfig> {
        const response = await this.client.get<any, NetworkTypes.NetworkConfig>('/machines/' + this.machine_id + '/networks/config/' + inst_id);
        return NetworkTypes.normalizeNetworkConfig(response);
    }
    async generate_config(config: NetworkTypes.NetworkConfig): Promise<Api.GenerateConfigResponse> {
        try {
            const response = await this.client.post<any, GenerateConfigResponse>('/generate-config', {
                config: NetworkTypes.toBackendNetworkConfig(config)
            });
            return response;
        } catch (error) {
            if (error instanceof AxiosError) {
                return { error: error.response?.data };
            }
            return { error: 'Unknown error: ' + error };
        }
    }
    async parse_config(toml_config: string): Promise<Api.ParseConfigResponse> {
        try {
            const response = await this.client.post<any, ParseConfigResponse>('/parse-config', { toml_config });
            if (response.config) {
                response.config = NetworkTypes.normalizeNetworkConfig(response.config);
            }
            return response;
        } catch (error) {
            if (error instanceof AxiosError) {
                return { error: error.response?.data };
            }
            return { error: 'Unknown error: ' + error };
        }
    }
    async get_network_metas(instance_ids: string[]): Promise<Api.GetNetworkMetasResponse> {
        const response = await this.client.post<any, Api.GetNetworkMetasResponse>(`/machines/${this.machine_id}/networks/metas`, {
            instance_ids: instance_ids
        });
        return response;
    }
}

export default ApiClient;
