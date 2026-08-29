<script setup lang="ts">
// "两步验证"弹窗：查看状态 / 绑定 / 解绑（解绑需输入当前动态码）
import { computed, inject, onMounted, ref } from 'vue';
import { Button, InputText, Message, Tag } from 'primevue';
import { useToast } from 'primevue/usetoast';
import TwoFactorSetupPanel from './TwoFactorSetupPanel.vue';
import ApiClient from '../modules/api';

const dialogRef = inject<any>('dialogRef');
const api = computed<ApiClient>(() => dialogRef.value.data.api);
const toast = useToast();

const status = ref<{ enabled: boolean; is_superuser: boolean; setup_required: boolean } | null>(null);
const code = ref('');
const loading = ref(false);

const loadStatus = async () => {
    try {
        status.value = await api.value.get2faStatus();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const onEnabled = () => {
    loadStatus();
    dialogRef.value.close();
};

const onDisable = async () => {
    if (!code.value.trim()) return;
    loading.value = true;
    try {
        await api.value.disable2fa(code.value.trim());
        toast.add({ severity: 'success', summary: '两步验证已关闭', life: 2000 });
        dialogRef.value.close();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '操作失败', detail: e?.response?.data?.message ?? e, life: 3000 });
        code.value = '';
    } finally {
        loading.value = false;
    }
};

onMounted(loadStatus);
</script>

<template>
    <Card class="w-full max-w-md p-6">
        <template #header>
            <h2 class="text-xl font-semibold">两步验证</h2>
        </template>
        <template #content>
            <div v-if="!status" class="text-sm text-gray-500">加载中…</div>

            <div v-else-if="!status.enabled" class="space-y-3">
                <p class="text-sm">为账号启用两步验证后，登录时需要输入验证器 App 的动态码。</p>
                <TwoFactorSetupPanel :api="api" @enabled="onEnabled" />
            </div>

            <div v-else class="space-y-3">
                <div class="flex items-center gap-2">
                    <Tag value="已启用" severity="success" />
                    <span class="text-sm text-gray-500">登录时需要输入验证器动态码</span>
                </div>
                <Message v-if="status.is_superuser" severity="warn" :closable="false" class="w-full">
                    管理员账号解绑后，下次进入管理后台将被要求重新绑定。
                </Message>
                <div class="p-field">
                    <label class="block text-sm font-medium mb-1">输入当前动态码以关闭两步验证</label>
                    <InputText v-model="code" maxlength="6" inputmode="numeric" placeholder="123456"
                        class="w-full" @keyup.enter="onDisable" />
                </div>
                <div class="flex gap-2">
                    <Button label="关闭两步验证" icon="pi pi-shield" severity="danger" :loading="loading"
                        @click="onDisable" class="flex-1" />
                    <Button label="取消" severity="secondary" @click="dialogRef.close()" />
                </div>
            </div>
        </template>
    </Card>
</template>
