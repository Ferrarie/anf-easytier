<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { Button, InputText } from 'primevue';
import { useToast } from 'primevue/usetoast';
import QRCode from 'qrcode';
import ApiClient from '../modules/api';

// TOTP 绑定面板：生成 secret → QR → 输码启用。绑定页与"两步验证"弹窗共用。
const props = defineProps<{ api: ApiClient }>();
const emit = defineEmits<{ (e: 'enabled'): void }>();

const toast = useToast();
const secret = ref('');
const qrSrc = ref('');
const code = ref('');
const loading = ref(false);

const loadSetup = async () => {
    loading.value = true;
    try {
        const ret = await props.api.setup2fa();
        secret.value = ret.secret;
        qrSrc.value = await QRCode.toDataURL(ret.otpauth_url, { width: 220, margin: 1 });
        code.value = '';
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '生成失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const onEnable = async () => {
    if (!code.value.trim()) return;
    loading.value = true;
    try {
        await props.api.enable2fa(code.value.trim());
        toast.add({ severity: 'success', summary: '两步验证已启用', life: 2000 });
        emit('enabled');
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '验证失败', detail: e?.response?.data?.message ?? e, life: 3000 });
        code.value = '';
    } finally {
        loading.value = false;
    }
};

onMounted(loadSetup);
</script>

<template>
    <div class="space-y-3">
        <p class="text-sm">用验证器 App（Google Authenticator、Microsoft Authenticator 等）扫描二维码，或手动输入密钥：</p>
        <div class="flex flex-col items-center gap-2">
            <img v-if="qrSrc" :src="qrSrc" alt="两步验证二维码" class="rounded border border-gray-200" />
            <code class="text-sm bg-gray-100 dark:bg-gray-700 rounded px-2 py-1 select-all break-all">{{ secret }}</code>
        </div>
        <div class="p-field">
            <label class="block text-sm font-medium mb-1">输入 App 显示的 6 位动态码完成绑定</label>
            <InputText v-model="code" maxlength="6" inputmode="numeric" placeholder="123456" class="w-full"
                @keyup.enter="onEnable" />
        </div>
        <div class="flex gap-2">
            <Button label="确认绑定" icon="pi pi-check" :loading="loading" @click="onEnable" class="flex-1" />
            <Button label="重新生成" icon="pi pi-refresh" severity="secondary" :disabled="loading"
                @click="loadSetup" />
        </div>
    </div>
</template>
