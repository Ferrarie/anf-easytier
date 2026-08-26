<script setup lang="ts">
import { onMounted, ref } from 'vue';
import {
    Button, Column, DataTable, Dialog, InputText, Tag, useToast,
} from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();
const networks = ref<Array<any>>([]);
const loading = ref(false);

const createDialog = ref(false);
const newName = ref('');
const newCidr = ref('');

const membersDialog = ref(false);
const members = ref<Array<any>>([]);
const membersTitle = ref('');

const load = async () => {
    loading.value = true;
    try {
        networks.value = (await props.api?.listNetworks()) ?? [];
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载网络失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const create = async () => {
    if (!newName.value.trim()) {
        toast.add({ severity: 'warn', summary: '名称必填', life: 2000 });
        return;
    }
    try {
        const created = await props.api?.createNetwork(newName.value.trim(), newCidr.value);
        toast.add({
            severity: 'success',
            summary: `网络已创建（网段 ${created?.cidr ?? '—'}）`,
            life: 2000,
        });
        newName.value = '';
        newCidr.value = '';
        createDialog.value = false;
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '创建失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const remove = async (id: string) => {
    try {
        await props.api?.deleteNetwork(id);
        toast.add({ severity: 'success', summary: '已删除', life: 2000 });
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '删除失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const showMembers = async (net: any) => {
    membersTitle.value = net.name;
    members.value = (await props.api?.networkDevices(net.id)) ?? [];
    membersDialog.value = true;
};

onMounted(load);
</script>

<template>
    <div>
        <div class="flex items-center gap-3 mb-4">
            <h1 class="text-xl font-semibold">网络管理</h1>
            <Button label="新建网络" icon="pi pi-plus" @click="createDialog = true" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="load" />
        </div>

        <DataTable :value="networks" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" />
            <Column field="name" header="名称" />
            <Column field="cidr" header="网段">
                <template #body="{ data }">{{ data.cidr ?? '-' }}</template>
            </Column>
            <Column field="device_count" header="成员数" style="width: 6rem" />
            <Column header="操作" style="width: 14rem">
                <template #body="{ data }">
                    <Button label="成员" size="small" severity="secondary" class="mr-1"
                        @click="showMembers(data)" />
                    <Button label="删除" size="small" severity="danger" @click="remove(data.id)" />
                </template>
            </Column>
        </DataTable>

        <Dialog v-model:visible="createDialog" header="新建网络" modal class="w-full max-w-md">
            <div class="space-y-4">
                <div class="p-field">
                    <label class="block text-sm font-medium">名称</label>
                    <InputText v-model="newName" class="w-full" required />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">网段（可选，留空自动分配随机网段，如 10.x.y.0/24）</label>
                    <InputText v-model="newCidr" class="w-full" placeholder="10.10.0.0/24" />
                </div>
                <div class="flex justify-end gap-2">
                    <Button label="取消" severity="secondary" @click="createDialog = false" />
                    <Button label="创建" @click="create" />
                </div>
            </div>
        </Dialog>

        <Dialog v-model:visible="membersDialog" :header="`成员：${membersTitle}`" modal class="w-full max-w-lg">
            <DataTable :value="members" striped-rows class="w-full">
                <Column field="display_name" header="显示名" />
                <Column field="machine_id" header="机器码">
                    <template #body="{ data }"><code class="text-xs">{{ data.machine_id }}</code></template>
                </Column>
                <Column field="status" header="状态">
                    <template #body="{ data }"><Tag :value="data.status" /></template>
                </Column>
            </DataTable>
        </Dialog>
    </div>
</template>
