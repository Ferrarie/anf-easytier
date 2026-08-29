<script setup lang="ts">
// 两步验证码输入页：login 返回 require_2fa 后进入；半会话 5 分钟内有效
import { computed, onMounted, ref } from 'vue';
import { Button, Card, InputText } from 'primevue';
import { useRouter } from 'vue-router';
import { useToast } from 'primevue/usetoast';
import ApiClient from '../modules/api';
import { getInitialApiHost } from '../modules/api-host';

const router = useRouter();
const toast = useToast();
// 不传 authFailedCb：动态码输错的 401 只弹提示，不踢回登录页
const api = computed<ApiClient>(() => new ApiClient(getInitialApiHost()));

const code = ref('');
const loading = ref(false);

const backToLogin = () => router.replace({ name: 'login' });

const onSubmit = async () => {
    if (!code.value.trim()) return;
    loading.value = true;
    try {
        const ret = await api.value.verify2fa(code.value.trim());
        if (ret.setup_required) {
            // superuser 强制绑定流程：验码通过（未绑定放行）后先完成绑定
            router.push({ name: 'twoFactorSetup' });
        } else {
            router.push({ name: 'dashboard', params: { apiHost: btoa(getInitialApiHost()) } });
        }
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '验证失败', detail: e?.response?.data?.message ?? e, life: 3000 });
        code.value = '';
        // 半会话可能已过期/作废：确认后回登录页
        try {
            const p = await api.value.get2faPending();
            if (!p.pending) backToLogin();
        } catch {
            backToLogin();
        }
    } finally {
        loading.value = false;
    }
};

onMounted(async () => {
    try {
        const p = await api.value.get2faPending();
        if (!p.pending) backToLogin();
    } catch {
        backToLogin();
    }
});
</script>

<template>
    <div class="flex items-center justify-center min-h-screen">
        <Card class="w-full max-w-md p-6">
            <template #header>
                <h2 class="text-2xl font-semibold text-center">两步验证</h2>
            </template>
            <template #content>
                <form @submit.prevent="onSubmit" class="space-y-4">
                    <p class="text-sm text-gray-500">输入验证器 App 中的 6 位动态码</p>
                    <InputText v-model="code" maxlength="6" inputmode="numeric" autofocus required
                        placeholder="123456"
                        class="w-full text-center text-xl tracking-widest" />
                    <Button label="验证" type="submit" class="w-full" :loading="loading" />
                    <Button label="返回登录" type="button" severity="secondary" class="w-full" @click="backToLogin" />
                </form>
            </template>
        </Card>
    </div>
</template>
