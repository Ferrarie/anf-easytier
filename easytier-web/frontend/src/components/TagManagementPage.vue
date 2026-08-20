<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { Button, Column, DataTable, InputText, useToast } from 'primevue';
import ApiClient from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();
const tags = ref<Array<any>>([]);
const loading = ref(false);
const newTag = ref('');

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

const create = async () => {
    try {
        await props.api?.createTag(newTag.value);
        toast.add({ severity: 'success', summary: 'tag 已创建', life: 2000 });
        newTag.value = '';
        await load();
    } catch (e: any) {
        toast.add({ severity: 'error', summary: '创建失败', detail: e?.response?.data?.message ?? e, life: 3000 });
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
            <InputText v-model="newTag" placeholder="新 tag 名（字母/数字/中划线）" class="w-64" @keyup.enter="create" />
            <Button label="创建" icon="pi pi-plus" @click="create" />
            <Button label="刷新" icon="pi pi-refresh" severity="secondary" @click="load" />
        </div>

        <DataTable :value="tags" :loading="loading" striped-rows class="w-full">
            <Column field="id" header="ID" style="width: 4rem" />
            <Column field="name" header="名称" />
            <Column field="used_by" header="引用设备数" style="width: 8rem" />
            <Column header="操作" style="width: 8rem">
                <template #body="{ data }">
                    <Button label="删除" size="small" severity="danger" @click="remove(data.id)" />
                </template>
            </Column>
        </DataTable>
    </div>
</template>
