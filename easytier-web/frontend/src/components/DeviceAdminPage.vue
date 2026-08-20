<script setup lang="ts">
import { onMounted, ref } from 'vue';
import {
    Button, Column, DataTable, Dialog, Dropdown, InputText, Tag, useToast,
} from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();

const devices = ref<Array<any>>([]);
const loading = ref(false);
const statusFilter = ref<string | undefined>(undefined);

const statusOptions = [
    { label: '全部', value: undefined },
    { label: '待审批', value: 'pending' },
    { label: '已放行', value: 'approved' },
    { label: '已拒绝', value: 'rejected' },
    { label: '已踢出', value: 'kicked' },
];

const load = async () => {
    loading.value = true;
    try {
        devices.value = (await props.api?.listDevices(statusFilter.value)) ?? [];
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载设备失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const action = async (fn: () => Promise<any>, ok: string) => {
    try {
        await fn();
        toast.add({ severity: 'success', summary: ok, life: 2000 });
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '操作失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const editDialog = ref(false);
const editingDevice = ref<any>(null);
const editName = ref('');
const editTags = ref('');
const editNetworks = ref('');

const openEdit = (device: any) => {
    editingDevice.value = device;
    editName.value = device.display_name ?? '';
    editTags.value = (device.tags ?? []).join(',');
    editNetworks.value = (device.networks ?? []).join(',');
    editDialog.value = true;
};

const saveEdit = async () => {
    await action(async () => props.api?.updateDevice(editingDevice.value.id, {
        display_name: editName.value,
        tags: editTags.value.split(',').map((s) => s.trim()).filter(Boolean),
        networks: editNetworks.value.split(',').map((s) => s.trim()).filter(Boolean),
    }), '已保存');
    editDialog.value = false;
};

const statusSeverity = (status: string) => {
    switch (status) {
        case 'approved': return 'success';
        case 'rejected': return 'danger';
        case 'kicked': return 'warn';
        default: return 'info';
    }
};

onMounted(load);
</script>

<template>
    <div>
        <div class="flex items-center gap-3 mb-4">
            <h1 class="text-xl font-semibold">设备审批</h1>
            <Dropdown v-model="statusFilter" :options="statusOptions" option-label="label" option-value="value"
                placeholder="状态筛选" class="w-40" @change="load" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="load" />
        </div>

        <DataTable :value="devices" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="display_name" header="显示名" />
            <Column field="machine_id" header="机器码" style="width: 16rem">
                <template #body="{ data }">
                    <code class="text-xs">{{ data.machine_id }}</code>
                </template>
            </Column>
            <Column field="status" header="状态" style="width: 6rem">
                <template #body="{ data }">
                    <Tag :value="data.status" :severity="statusSeverity(data.status)" />
                </template>
            </Column>
            <Column header="Tag" style="width: 10rem">
                <template #body="{ data }">
                    <Tag v-for="tag in data.tags" :key="tag" :value="tag" severity="secondary" class="mr-1" />
                </template>
            </Column>
            <Column header="网络" style="width: 10rem">
                <template #body="{ data }">
                    <span v-for="net in data.networks" :key="net" class="mr-1 text-xs">{{ net }}</span>
                </template>
            </Column>
            <Column header="操作" style="width: 18rem">
                <template #body="{ data }">
                    <Button v-if="data.status === 'pending'" label="放行" size="small" severity="success" class="mr-1"
                        @click="action(async () => { await props.api?.approveDevice(data.id) }, '已放行')" />
                    <Button v-if="data.status === 'pending' || data.status === 'approved'" label="拒绝" size="small"
                        severity="danger" class="mr-1"
                        @click="action(async () => { await props.api?.rejectDevice(data.id) }, '已拒绝')" />
                    <Button v-if="data.status === 'approved'" label="踢出" size="small" severity="warn" class="mr-1"
                        @click="action(async () => { await props.api?.kickDevice(data.id) }, '已踢出')" />
                    <Button label="编辑" size="small" severity="secondary"
                        @click="openEdit(data)" />
                </template>
            </Column>
        </DataTable>

        <Dialog v-model:visible="editDialog" header="编辑设备" modal class="w-full max-w-md">
            <div class="space-y-4">
                <div class="p-field">
                    <label class="block text-sm font-medium">显示名</label>
                    <InputText v-model="editName" class="w-full" />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">Tag（逗号分隔）</label>
                    <InputText v-model="editTags" class="w-full" placeholder="办公,服务器" />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">网络实例（逗号分隔）</label>
                    <InputText v-model="editNetworks" class="w-full" placeholder="net-xxxx" />
                </div>
                <div class="flex justify-end gap-2">
                    <Button label="取消" severity="secondary" @click="editDialog = false" />
                    <Button label="保存" @click="saveEdit" />
                </div>
            </div>
        </Dialog>
    </div>
</template>
