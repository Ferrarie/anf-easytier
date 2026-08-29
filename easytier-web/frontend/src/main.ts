import { createApp } from 'vue'
import 'easytier-frontend-lib/style.css'
import './style.css'
import App from './App.vue'
import EasytierFrontendLib from 'easytier-frontend-lib'
import PrimeVue from 'primevue/config'
import Aura from '@primeuix/themes/aura';
import { definePreset } from '@primeuix/themes';
import ConfirmationService from 'primevue/confirmationservice';
import { I18nUtils } from 'easytier-frontend-lib'

import { createRouter, createWebHashHistory } from 'vue-router'
import MainPage from './components/MainPage.vue'
import Login from './components/Login.vue'
import DeviceList from './components/DeviceList.vue'
import DeviceManagement from './components/DeviceManagement.vue'
import Dashboard from './components/Dashboard.vue'
import DeviceAdminPage from './components/DeviceAdminPage.vue'
import InviteManagementPage from './components/InviteManagementPage.vue'
import RegisterDevice from './components/RegisterDevice.vue'
import NetworkManagementPage from './components/NetworkManagementPage.vue'
import TagManagementPage from './components/TagManagementPage.vue'
import AclEditorPage from './components/AclEditorPage.vue'
import TwoFactorPage from './components/TwoFactorPage.vue'
import TwoFactorSetup from './components/TwoFactorSetup.vue'
import UserAdminPage from './components/UserAdminPage.vue'
import DialogService from 'primevue/dialogservice';
import ToastService from 'primevue/toastservice';
import ApiClient from './modules/api';

// ANF 品牌主色：indigo（Tailwind indigo 色阶，与 GUI 端 #6366f1→#8b5cf6 渐变同族）
const AnfPreset = definePreset(Aura, {
    semantic: {
        primary: {
            50: '#eef2ff',
            100: '#e0e7ff',
            200: '#c7d2fe',
            300: '#a5b4fc',
            400: '#818cf8',
            500: '#6366f1',
            600: '#4f46e5',
            700: '#4338ca',
            800: '#3730a3',
            900: '#312e81',
            950: '#1e1b4b',
        },
    },
})

const routes = [
    {
        path: '/auth', children: [
            {
                name: 'login',
                path: '',
                component: Login,
                alias: 'login',
                props: { isRegistering: false }
            },
            {
                name: 'register',
                path: 'register',
                component: Login,
                props: { isRegistering: true }
            },
            {
                name: 'deviceRegister',
                path: 'device-register',
                component: RegisterDevice,
            },
            {
                name: 'twoFactor',
                path: '2fa',
                component: TwoFactorPage,
            },
            {
                name: 'twoFactorSetup',
                path: '2fa/setup',
                component: TwoFactorSetup,
            }
        ]
    },
    {
        path: '/h/:apiHost', component: MainPage, children: [
            {
                path: '',
                alias: 'dashboard',
                name: 'dashboard',
                component: Dashboard,
            },
            {
                path: 'deviceList',
                name: 'deviceList',
                component: DeviceList,
                children: [
                    {
                        path: 'device/:deviceId/:instanceId?',
                        name: 'deviceManagement',
                        component: DeviceManagement,
                    }
                ]
            },
            {
                path: 'devices',
                name: 'deviceAdmin',
                component: DeviceAdminPage,
            },
            {
                path: 'invites',
                name: 'inviteManagement',
                component: InviteManagementPage,
            },
            {
                path: 'networks',
                name: 'networkManagement',
                component: NetworkManagementPage,
            },
            {
                path: 'tags',
                name: 'tagManagement',
                component: TagManagementPage,
            },
            {
                path: 'acl',
                name: 'aclEditor',
                component: AclEditorPage,
            },
            {
                path: 'users',
                name: 'userAdmin',
                component: UserAdminPage,
            },
        ]
    },
    {
        path: '/:pathMatch(.*)*', name: 'notFound', redirect: () => {
            let apiHost = localStorage.getItem('apiHost');
            if (apiHost) {
                return { name: 'dashboard', params: { apiHost: apiHost } }
            } else {
                return { name: 'login' }
            }
        }
    }
]

const router = createRouter({
    history: createWebHashHistory(),
    routes,
})

// 登录态守卫：进入 /h/:apiHost （及子路由）前，用后端会话 cookie 校验真实登录态，
// 避免 localStorage 残留 apiHost 时“退出后新开页面仍显示已登录”。
router.beforeEach(async (to) => {
    if (to.path.startsWith('/h/')) {
        const apiHost = to.params.apiHost as string | undefined;
        if (!apiHost) {
            return { name: 'login' };
        }
        let host: string;
        try {
            host = atob(apiHost);
        } catch {
            return { name: 'login' };
        }
        try {
            const client = new ApiClient(host);
            const status = await client.check_login_status_detail();
            if (!status.logged_in) {
                return { name: 'login' };
            }
            // superuser 强制两步验证：未绑定先完成绑定，进不了任何功能页
            if (status.require_two_factor_setup) {
                return { name: 'twoFactorSetup' };
            }
        } catch {
            return { name: 'login' };
        }
    }
    // 其余路由放行
    return true;
});

const app = createApp(App)

// Use i18n
app.use(I18nUtils.i18n)

app.use(PrimeVue,
    {
        theme: {
            preset: AnfPreset,
            options: {
                prefix: 'p',
                darkModeSelector: 'system',
                cssLayer: {
                    name: 'primevue',
                    order: 'tailwind-base, primevue, tailwind-utilities'
                }
            }
        }
    }
).use(ToastService as any).use(DialogService as any).use(router).use(ConfirmationService as any).use(EasytierFrontendLib).mount('#app')
