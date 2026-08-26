<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Button, Column, DataTable, Dialog, InputText, useToast } from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();
const tags = ref<Array<any>>([]);
const loading = ref(false);

const createDialog = ref(false);
const editId = ref<number | undefined>(undefined);
const newName = ref('');
const dialogTitle = computed(() => (editId.value ? '编辑 Tag' : '新建 Tag'));

const load = async () => {
    loading.value = true;
    try {
        tags.value = (await props.api?.listTags()) ?? [];
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '加载 tag 失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    } finally {
        loading.value = false;
    }
};

const openCreate = () => {
    editId.value = undefined;
    newName.value = '';
    createDialog.value = true;
};

const openEdit = (tag: any) => {
    editId.value = tag.id;
    newName.value = tag.name;
    createDialog.value = true;
};

const save = async () => {
    if (!newName.value.trim()) {
        toast.add({ severity: 'warn', summary: '名称必填', life: 2000 });
        return;
    }
    try {
        if (editId.value) {
            await props.api?.updateTag(editId.value, newName.value.trim());
            toast.add({ severity: 'success', summary: 'tag 已更新', life: 2000 });
        } else {
            await props.api?.createTag(newName.value.trim());
            toast.add({ severity: 'success', summary: 'tag 已创建', life: 2000 });
        }
        createDialog.value = false;
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '保存失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

const remove = async (id: number) => {
    try {
        await props.api?.deleteTag(id);
        toast.add({ severity: 'success', summary: '已删除', life: 2000 });
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '删除失败', detail: e?.response?.data?.message ?? e, life: 3000 });
    }
};

onMounted(load);
</script>

<template>
    <div>
        <div class="flex items-center gap-3 mb-4">
            <h1 class="text-xl font-semibold">Tag 管理</h1>
            <Button label="新建 Tag" icon="pi pi-plus" @click="openCreate" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="load" />
        </div>

        <DataTable :value="tags" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="name" header="名称" />
            <Column field="used_by" header="引用设备数" style="width: 8rem" />
            <Column header="操作" style="width: 12rem">
                <template #body="{ data }">
                    <Button label="编辑" size="small" severity="secondary" class="mr-1" @click="openEdit(data)" />
                    <Button label="删除" size="small" severity="danger" @click="remove(data.id)" />
                </template>
            </Column>
        </DataTable>

        <Dialog v-model:visible="createDialog" :header="dialogTitle" modal class="w-full max-w-md">
            <div class="space-y-4">
                <div v-if="editId" class="p-field">
                    <label class="block text-sm font-medium">ID</label>
                    <InputText :model-value="String(editId)" class="w-full" disabled />
                </div>
                <div class="p-field">
                    <label class="block text-sm font-medium">名称</label>
                    <InputText v-model="newName" class="w-full" placeholder="字母/数字/中划线/下划线/点" />
                </div>
                <div class="flex justify-end gap-2">
                    <Button label="取消" severity="secondary" @click="createDialog = false" />
                    <Button label="保存" @click="save" />
                </div>
            </div>
        </Dialog>
    </div>
</template>
