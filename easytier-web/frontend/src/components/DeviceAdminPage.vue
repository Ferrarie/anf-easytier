<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
    Button, Column, DataTable, Dialog, Dropdown, MultiSelect, InputText, Tag, useToast,
} from 'primevue';
import ApiClient from '../modules/api';
import { toStringArray } from '../utils/stringArray';

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
const editTags = ref<Array<string>>([]);
const editNetworks = ref<Array<string>>([]);

// 下拉多选候选：tag 名 / 网络实例 id
const tagOptions = ref<Array<{ label: string; value: string }>>([]);
const networkOptions = ref<Array<{ label: string; value: string }>>([]);

const approveGuideVisible = ref(false);
const guideDevice = ref<any>(null);
const guideMissing = computed(() => {
    const d = guideDevice.value;
    if (!d) return [];
    const missing = [];
    if (!(Array.isArray(d.tags) && d.tags.length)) missing.push('Tag');
    if (!(Array.isArray(d.networks) && d.networks.length)) missing.push('网络');
    return missing;
});

const doApprove = (device: any) => action(async () => props.api?.approveDevice(device.id), '已放行');

/** 放行前先检查是否已分配 Tag 与网络；缺失则引导去分配（Polanyi：把隐性依赖显性化）。 */
const onApprove = (device: any) => {
    const hasTags = Array.isArray(device.tags) && device.tags.length > 0;
    const hasNets = Array.isArray(device.networks) && device.networks.length > 0;
    if (hasTags && hasNets) {
        void doApprove(device);
        return;
    }
    guideDevice.value = device;
    approveGuideVisible.value = true;
};

const goAssign = () => {
    const d = guideDevice.value;
    approveGuideVisible.value = false;
    guideDevice.value = null;
    if (d) openEdit(d);
};

const continueApprove = () => {
    const d = guideDevice.value;
    approveGuideVisible.value = false;
    guideDevice.value = null;
    if (d) void doApprove(d);
};

const closeGuide = () => {
    approveGuideVisible.value = false;
    guideDevice.value = null;
};

const openEdit = (device: any) => {
    editingDevice.value = device;
    editName.value = device.display_name ?? '';
    editTags.value = device.tags ?? [];
    editNetworks.value = device.networks ?? [];
    editDialog.value = true;
};

const saveEdit = async () => {
    await action(async () => props.api?.updateDevice(editingDevice.value.id, {
        display_name: editName.value,
        tags: toStringArray(editTags.value),
        networks: toStringArray(editNetworks.value),
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

/** 加载 tag / 网络候选列表（用于编辑弹窗下拉多选）。 */
const loadOptions = async () => {
    try {
        const tags = (await props.api?.listTags()) ?? [];
        tagOptions.value = tags.map((t: any) => ({ label: t.name, value: t.name }));

        const nets = (await props.api?.listNetworks()) ?? [];
        networkOptions.value = nets.map((n: any) => ({ label: `${n.name}（${n.id}）`, value: n.id }));
    } catch (e: any) {
        // 候选列表失败不阻塞编辑，允许手动输入（保底）
        console.warn('failed to load tag/network options', e);
    }
};

const networkLabel = (id: string) => {
    const hit = networkOptions.value.find((n) => n.value === id);
    return hit ? hit.label : id;
};

const deleteDevice = async (device: any) => {
    if (!window.confirm(`确认删除设备「${device.display_name}」吗？该操作不可恢复。`)) {
        return;
    }
    await action(async () => props.api?.deleteDevice(device.id), '已删除');
};

onMounted(async () => {
    await Promise.all([load(), loadOptions()]);
});
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
            <Column field="display_name" header="显示名">
                <template #body="{ data }">
                    <div class="flex items-center gap-2">
                        <span>{{ data.display_name }}</span>
                        <Tag v-if="!(Array.isArray(data.tags) && data.tags.length) || !(Array.isArray(data.networks) && data.networks.length)"
                            value="未分配Tag/网络" severity="warn" />
                    </div>
                </template>
            </Column>
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
                    <span v-for="net in data.networks" :key="net" class="mr-1 text-xs">{{ networkLabel(net) }}</span>
                </template>
            </Column>
            <Column header="操作" style="width: 18rem">
                <template #body="{ data }">
                    <Button v-if="data.status === 'pending'" label="放行" size="small" severity="success" class="mr-1"
                        @click="onApprove(data)" />
                    <Button v-if="data.status === 'pending' || data.status === 'approved'" label="拒绝" size="small"
                        severity="danger" class="mr-1"
                        @click="action(async () => { await props.api?.rejectDevice(data.id) }, '已拒绝')" />
                    <Button v-if="data.status === 'approved'" label="踢出" size="small" severity="warn" class="mr-1"
                        @click="action(async () => { await props.api?.kickDevice(data.id) }, '已踢出')" />
                    <Button label="编辑" size="small" severity="secondary"
                        @click="openEdit(data)" />
                    <Button label="删除" size="small" severity="danger" outlined class="mr-1"
                        @click="deleteDevice(data)" />
                </template>
            </Column>
        </DataTable>

        <Dialog v-model:visible="approveGuideVisible" header="放行前需先分配" modal class="w-full max-w-md">
            <div class="space-y-3 text-sm">
                <p>
                    该设备尚未分配
                    <span v-for="m in guideMissing" :key="m" class="font-medium text-amber-600">{{ m }}</span>。
                    未分配时放行将无法生成托管配置（虚拟 IP / ACL）。
                </p>
                <p class="text-secondary">
                    请先到「设备 → 编辑」分配 Tag 与网络，或先新建设备网络后再放行。
                </p>
            </div>
            <template #footer>
                <Button label="取消" severity="text" @click="closeGuide" />
                <Button label="仍要放行" severity="secondary" @click="continueApprove" />
                <Button label="去分配" @click="goAssign" />
            </template>
        </Dialog>

        <Dialog v-model:visible="editDialog" header="编辑设备" modal class="w-full max-w-md">
            <div class="space-y-4">
                <div class="p-field">
                    <label class="block text-sm font-medium">显示名</label>
                    <InputText v-model="editName" class="w-full" />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">Tag</label>
                    <MultiSelect v-model="editTags" :options="tagOptions" option-label="label" option-value="value"
                        placeholder="选择 Tag（可多选）" class="w-full" :showClear="true" :filter="true" />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">网络实例</label>
                    <MultiSelect v-model="editNetworks" :options="networkOptions" option-label="label" option-value="value"
                        placeholder="选择网络实例（可多选）" class="w-full" :showClear="true" :filter="true" />
                </div>
                <div class="flex justify-end gap-2">
                    <Button label="取消" severity="secondary" @click="editDialog = false" />
                    <Button label="保存" @click="saveEdit" />
                </div>
            </div>
        </Dialog>
    </div>
</template>
