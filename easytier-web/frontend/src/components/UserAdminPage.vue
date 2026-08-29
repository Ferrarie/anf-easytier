<script setup lang="ts">
// 用户管理（superuser 专用）：用户列表 + 2FA 状态 + 重置两步验证
import { onMounted, ref } from 'vue';
import { Button, Column, DataTable, Tag } from 'primevue';
import { useToast } from 'primevue/usetoast';
import ApiClient from '../modules/api';

const props = defineProps({
    api: { type: ApiClient, required: true },
});

const toast = useToast();
const users = ref<Array<any>>([]);
const loading = ref(false);

const load = async () => {
    loading.value = true;
    try {
        users.value = await props.api.adminListUsers();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const reset2fa = async (u: any) => {
    if (!window.confirm(`确定重置用户「${u.username}」的两步验证？其绑定信息与锁定状态将被清除，下次登录需重新绑定。`)) {
        return;
    }
    try {
        await props.api.adminReset2fa(u.id);
        toast.add({ severity: 'success', summary: '已重置', detail: u.username, life: 2000 });
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '重置失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

onMounted(load);
</script>

<template>
    <div>
        <h2 class="text-xl font-semibold mb-3">用户管理</h2>
        <DataTable :value="users" :loading="loading" striped-rows dataKey="id">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="username" header="用户名" />
            <Column header="角色" style="width: 8rem">
                <template #body="{ data }">
                    <Tag v-if="data.is_superuser" value="管理员" severity="warn" />
                    <Tag v-else value="普通用户" severity="info" />
                </template>
            </Column>
            <Column header="两步验证" style="width: 8rem">
                <template #body="{ data }">
                    <Tag v-if="data.totp_enabled" value="已启用" severity="success" />
                    <Tag v-else value="未启用" severity="danger" />
                </template>
            </Column>
            <Column header="操作" style="width: 10rem">
                <template #body="{ data }">
                    <Button label="重置两步验证" icon="pi pi-shield" size="small" severity="danger" text
                        @click="reset2fa(data)" />
                </template>
            </Column>
        </DataTable>
    </div>
</template>
