<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
    Button, Checkbox, Column, DataTable, Dialog, Dropdown, InputNumber, InputText,
    MultiSelect, useToast,
} from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();

const networks = ref<Array<any>>([]);
const selectedNetworkId = ref<string | undefined>(undefined);
const rules = ref<Array<any>>([]);
const allTags = ref<Array<string>>([]);
const loading = ref(false);

const selectedNetwork = computed(() =>
    networks.value.find((n) => n.id === selectedNetworkId.value),
);

const loadNetworks = async () => {
    networks.value = (await props.api?.listNetworks()) ?? [];
    const tags = (await props.api?.listTags()) ?? [];
    allTags.value = tags.map((t: any) => t.name);
    if (!selectedNetworkId.value && networks.value.length > 0) {
        selectedNetworkId.value = networks.value[0].id;
    }
};

const loadRules = async () => {
    if (!selectedNetworkId.value) return;
    loading.value = true;
    try {
        rules.value = (await props.api?.listAclRules(selectedNetworkId.value)) ?? [];
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载规则失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const ruleDialog = ref(false);
const editRuleId = ref<number | undefined>(undefined);
const ruleName = ref('');
const ruleSource = ref<Array<string>>([]);
const ruleDest = ref<Array<string>>([]);
const ruleProtocol = ref('any');
const rulePorts = ref('');
const ruleAction = ref('allow');
const rulePriority = ref(0);
const ruleEnabled = ref(true);

const ruleDialogTitle = computed(() => (editRuleId.value ? '编辑 ACL 规则' : '新建 ACL 规则'));

const protocolOptions = [
    { label: 'any', value: 'any' },
    { label: 'tcp', value: 'tcp' },
    { label: 'udp', value: 'udp' },
    { label: 'icmp', value: 'icmp' },
];

const actionOptions = [
    { label: '放行 allow', value: 'allow' },
    { label: '拒绝 drop', value: 'drop' },
];

const openCreate = () => {
    editRuleId.value = undefined;
    ruleName.value = '';
    ruleSource.value = [];
    ruleDest.value = [];
    ruleProtocol.value = 'any';
    rulePorts.value = '';
    ruleAction.value = 'allow';
    rulePriority.value = 0;
    ruleEnabled.value = true;
    ruleDialog.value = true;
};

const openEdit = (rule: any) => {
    editRuleId.value = rule.id;
    ruleName.value = rule.name;
    ruleSource.value = rule.source_tags ?? [];
    ruleDest.value = rule.destination_tags ?? [];
    ruleProtocol.value = rule.protocol ?? 'any';
    rulePorts.value = (rule.ports ?? []).join(',');
    ruleAction.value = rule.action ?? 'allow';
    rulePriority.value = rule.priority ?? 0;
    ruleEnabled.value = rule.enabled !== false;
    ruleDialog.value = true;
};

const save = async () => {
    if (!selectedNetworkId.value) return;
    const payload = {
        name: ruleName.value.trim(),
        enabled: ruleEnabled.value,
        source_tags: ruleSource.value,
        destination_tags: ruleDest.value,
        protocol: ruleProtocol.value,
        ports: rulePorts.value.split(',').map((s) => s.trim()).filter(Boolean),
        action: ruleAction.value,
        priority: rulePriority.value,
    };
    try {
        if (editRuleId.value) {
            await props.api?.updateAclRule(selectedNetworkId.value, editRuleId.value, payload);
            toast.add({ severity: 'success', summary: '规则已更新', life: 2000 });
        } else {
            await props.api?.createAclRule(selectedNetworkId.value, payload);
            toast.add({ severity: 'success', summary: '规则已创建', life: 2000 });
        }
        ruleDialog.value = false;
        await loadRules();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '保存失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const remove = async (ruleId: number) => {
    if (!selectedNetworkId.value) return;
    try {
        await props.api?.deleteAclRule(selectedNetworkId.value, ruleId);
        toast.add({ severity: 'success', summary: '已删除', life: 2000 });
        await loadRules();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '删除失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

onMounted(async () => {
    await loadNetworks();
    await loadRules();
});
</script>

<template>
    <div>
        <div class="flex items-center gap-3 mb-4">
            <h1 class="text-xl font-semibold">ACL 规则（默认拒绝）</h1>
            <Dropdown v-model="selectedNetworkId" :options="networks" option-label="name" option-value="id"
                placeholder="选择网络" class="w-56" @change="loadRules" />
            <Button label="新建规则" icon="pi pi-plus" :disabled="!selectedNetworkId" @click="openCreate" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="loadRules" />
        </div>

        <p class="text-sm text-gray-500 mb-3">
            当前网络：{{ selectedNetwork?.name ?? '-' }}（无匹配规则时默认拒绝）
        </p>

        <DataTable :value="rules" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="name" header="规则名" />
            <Column field="source_tags" header="源 tag">
                <template #body="{ data }">{{ data.source_tags.join(', ') || 'any' }}</template>
            </Column>
            <Column field="destination_tags" header="目标 tag">
                <template #body="{ data }">{{ data.destination_tags.join(', ') || 'any' }}</template>
            </Column>
            <Column field="protocol" header="协议" style="width: 5rem" />
            <Column field="ports" header="端口">
                <template #body="{ data }">{{ data.ports.join(', ') || 'any' }}</template>
            </Column>
            <Column field="action" header="动作" style="width: 5rem">
                <template #body="{ data }">
                    <span :class="data.action === 'allow' ? 'text-green-600' : 'text-red-500'">
                        {{ data.action }}
                    </span>
                </template>
            </Column>
            <Column field="priority" header="优先级" style="width: 6rem" />
            <Column field="enabled" header="启用" style="width: 5rem">
                <template #body="{ data }">
                    {{ data.enabled ? '是' : '否' }}
                </template>
            </Column>
            <Column header="操作" style="width: 12rem">
                <template #body="{ data }">
                    <Button label="编辑" size="small" severity="secondary" class="mr-1" @click="openEdit(data)" />
                    <Button label="删除" size="small" severity="danger" @click="remove(data.id)" />
                </template>
            </Column>
        </DataTable>

        <Dialog v-model:visible="ruleDialog" :header="ruleDialogTitle" modal class="w-full max-w-lg">
            <div class="space-y-4">
                <div v-if="editRuleId" class="p-field">
                    <label class="block text-sm font-medium">ID</label>
                    <InputText :model-value="String(editRuleId)" class="w-full" disabled />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">规则名</label>
                    <InputText v-model="ruleName" class="w-full" required />
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <div class="p-field">
                        <label class="block text-sm font-medium">源 tag（可多选）</label>
                        <MultiSelect v-model="ruleSource" :options="allTags" class="w-full" />
                    </div>
                    <div class="p-field">
                        <label class="block text-sm font-medium">目标 tag（可多选）</label>
                        <MultiSelect v-model="ruleDest" :options="allTags" class="w-full" />
                    </div>
                </div>
                <div class="grid grid-cols-3 gap-3">
                    <div class="p-field">
                        <label class="block text-sm font-medium">协议</label>
                        <Dropdown v-model="ruleProtocol" :options="protocolOptions" option-label="label"
                            option-value="value" class="w-full" />
                    </div>
                    <div class="p-field">
                        <label class="block text-sm font-medium">端口（逗号分隔）</label>
                        <InputText v-model="rulePorts" class="w-full" placeholder="80,443" />
                    </div>
                    <div class="p-field">
                        <label class="block text-sm font-medium">优先级（大者优先）</label>
                        <InputNumber v-model="rulePriority" :min="0" class="w-full" />
                    </div>
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">动作</label>
                    <Dropdown v-model="ruleAction" :options="actionOptions" option-label="label"
                        option-value="value" class="w-full" />
                </div>
                <div class="p-field flex items-center gap-2">
                    <Checkbox v-model="ruleEnabled" :binary="true" input-id="rule-enabled" />
                    <label for="rule-enabled" class="text-sm font-medium">启用该规则</label>
                </div>
                <div class="flex justify-end gap-2">
                    <Button label="取消" severity="secondary" @click="ruleDialog = false" />
                    <Button label="保存" @click="save" />
                </div>
            </div>
        </Dialog>
    </div>
</template>
