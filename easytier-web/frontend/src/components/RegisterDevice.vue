<script setup lang="ts">
import { ref } from 'vue';
import { Button, Card, InputText } from 'primevue';
import { useRouter } from 'vue-router';
import { useToast } from 'primevue/usetoast';
import ApiClient from '../modules/api';
import { getInitialApiHost, saveApiHost } from '../modules/api-host';

const router = useRouter();
const toast = useToast();

const apiHost = ref<string>(getInitialApiHost());
const inviteCode = ref('');
const machineId = ref<string>(crypto.randomUUID());

const api = new ApiClient(apiHost.value);

const onSubmit = async () => {
    saveApiHost(apiHost.value);
    try {
        const device = await api.registerDevice(inviteCode.value.trim(), machineId.value.trim());
        toast.add({
            severity: 'success',
            summary: '注册成功',
            detail: `设备 ${device.display_name} 已进入待审批，等待管理员放行。`,
            life: 4000,
        });
        router.push({ name: 'login' });
    } catch (e: any) {
        const msg = e?.response?.data?.message ?? String(e);
        toast.add({ severity: 'error', summary: '注册失败', detail: msg, life: 4000 });
    }
};
</script>

<template>
    <div class="flex items-center justify-center min-h-screen">
        <Card class="w-full max-w-md p-6">
            <template #header>
                <h2 class="text-2xl font-semibold text-center">设备注册（邀请码）</h2>
            </template>
            <template #content>
                <form @submit.prevent="onSubmit" class="space-y-4">
                    <div class="p-field">
                        <label class="block text-sm font-medium">中心地址（API Host）</label>
                        <InputText v-model="apiHost" required class="w-full" placeholder="http://10.144.x.1:11211" />
                    </div>
                    <div class="p-field">
                        <label class="block text-sm font-medium">邀请码</label>
                        <InputText v-model="inviteCode" required class="w-full" placeholder="12 位邀请码" />
                    </div>
                    <div class="p-field">
                        <label class="block text-sm font-medium">机器码（machine-id，默认随机生成）</label>
                        <InputText v-model="machineId" required class="w-full" />
                    </div>
                    <Button label="提交注册" type="submit" class="w-full" />
                    <Button label="返回登录" type="button" class="w-full" severity="secondary"
                        @click="router.push({ name: 'login' })" />
                </form>
            </template>
        </Card>
    </div>
</template>
