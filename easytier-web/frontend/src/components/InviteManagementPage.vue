<script setup lang="ts">
import { onMounted, ref } from 'vue';
import {
    Button, Column, DataTable, InputNumber, InputText, useToast,
} from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();
const invites = ref<Array<any>>([]);
const loading = ref(false);

const maxUses = ref(1);
const expiresAt = ref('');

const load = async () => {
    loading.value = true;
    try {
        invites.value = (await props.api?.listInvites()) ?? [];
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载邀请码失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const create = async () => {
    try {
        const invite = await props.api?.createInvite(maxUses.value, expiresAt.value || undefined);
        toast.add({ severity: 'success', summary: '已生成邀请码', detail: invite.code, life: 5000 });
        maxUses.value = 1;
        expiresAt.value = '';
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '生成失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const revoke = async (id: number) => {
    try {
        await props.api?.disableInvite(id);
        toast.add({ severity: 'success', summary: '已吊销', life: 2000 });
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '吊销失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

onMounted(load);
</script>

<template>
    <div>
        <h1 class="text-xl font-semibold mb-4">邀请码管理</h1>

        <div class="flex items-end gap-3 mb-6 p-4 border rounded-lg">
            <div class="p-field">
                <label class="block text-sm font-medium">最大使用次数</label>
                <InputNumber v-model="maxUses" :min="1" class="w-32" />
            </div>
            <div class="p-field">
                <label class="block text-sm font-medium">过期时间（可留空）</label>
                <InputText v-model="expiresAt" type="datetime-local" class="w-64" />
            </div>
            <Button label="生成邀请码" icon="pi pi-plus" @click="create" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="load" />
        </div>

        <DataTable :value="invites" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="code" header="邀请码" />
            <Column field="max_uses" header="最大次数" style="width: 6rem" />
            <Column field="used_count" header="已用" style="width: 6rem" />
            <Column field="enabled" header="状态" style="width: 6rem">
                <template #body="{ data }">
                    <span :class="data.enabled ? 'text-green-600' : 'text-red-500'">
                        {{ data.enabled ? '有效' : '已吊销' }}
                    </span>
                </template>
            </Column>
            <Column field="expires_at" header="过期时间" style="width: 12rem">
                <template #body="{ data }">{{ data.expires_at ?? '不过期' }}</template>
            </Column>
            <Column header="操作" style="width: 8rem">
                <template #body="{ data }">
                    <Button v-if="data.enabled" label="吊销" size="small" severity="danger"
                        @click="revoke(data.id)" />
                </template>
            </Column>
        </DataTable>
    </div>
</template>
