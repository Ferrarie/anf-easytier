<script setup lang="ts">
import { Card, useToast } from 'primevue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { Utils } from 'easytier-frontend-lib';
import ApiClient, { Summary } from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();

const summary = ref<Summary | undefined>(undefined);

const loadSummary = async () => {
    const resp = await props.api?.get_summary();
    summary.value = resp;
};

const periodFunc = new Utils.PeriodicTask(async () => {
    try {
        await loadSummary();
    } catch (e) {
        toast.add({ severity: 'error', summary: 'Load Summary Failed', detail: e, life: 2000 });
        console.error(e);
    }
}, 1000);

onMounted(async () => {
    periodFunc.start();
});

onUnmounted(() => {
    periodFunc.stop();
});

const deviceCount = computed<number | undefined>(
    () => {
        return summary.value?.device_count;
    },
);

</script>

<template>
    <div class="grid grid-cols-1 gap-4">
        <Card class="w-full">
            <template #title>Device Count</template>
            <template #content>
                <div class="w-full flex justify-center text-7xl font-bold text-green-800 mt-4">
                    {{ deviceCount }}
                </div>
            </template>
        </Card>
    </div>

</template>
