<script setup lang="ts">
// 绑定两步验证页：superuser 未绑定时被强制引导到此；普通用户也可直接访问
import { computed } from 'vue';
import { Card } from 'primevue';
import { useRouter } from 'vue-router';
import TwoFactorSetupPanel from './TwoFactorSetupPanel.vue';
import ApiClient from '../modules/api';
import { getInitialApiHost } from '../modules/api-host';

const router = useRouter();
const api = computed<ApiClient>(() => new ApiClient(getInitialApiHost()));

const onEnabled = () => {
    router.push({ name: 'dashboard', params: { apiHost: btoa(getInitialApiHost()) } });
};
</script>

<template>
    <div class="flex items-center justify-center min-h-screen">
        <Card class="w-full max-w-md p-6">
            <template #header>
                <h2 class="text-2xl font-semibold text-center">绑定两步验证</h2>
            </template>
            <template #content>
                <p class="text-sm text-gray-500 mb-3">
                    管理员账号已强制开启两步验证，完成绑定后才能进入管理后台；绑定后登录需输入验证器动态码。
                </p>
                <TwoFactorSetupPanel :api="api" @enabled="onEnabled" />
            </template>
        </Card>
    </div>
</template>
