<script setup lang="ts">
import { Button, Card, useToast } from 'primevue';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { Utils } from 'easytier-frontend-lib';
import ApiClient, { AnfStats, CenterInfo } from '../modules/api';

const props = defineProps({
    api: ApiClient,
});

const toast = useToast();

const centerInfo = ref<CenterInfo | undefined>(undefined);
const stats = ref<AnfStats | undefined>(undefined);

const loadCenterInfo = async () => {
    centerInfo.value = await props.api?.centerInfo();
};

const loadStats = async () => {
    stats.value = await props.api?.anfStats();
};

const periodFunc = new Utils.PeriodicTask(async () => {
    try {
        await loadStats();
    } catch (e) {
        console.error(e);
    }
}, 10000);

onMounted(async () => {
    try {
        await Promise.all([loadCenterInfo(), loadStats()]);
    } catch (e) {
        console.error(e);
    }
    periodFunc.start();
});

onUnmounted(() => {
    periodFunc.stop();
});

const host = window.location.hostname;

const configServerAddress = computed<string>(() => {
    const port = centerInfo.value?.config_server_port;
    return port ? `${host}:${port}` : '';
});

const peerPort = (url: string): string => {
    const m = url.match(/:(\d+)/);
    return m ? m[1] : '';
};

const copy = async (value: string) => {
    try {
        await navigator.clipboard.writeText(value);
        toast.add({ severity: 'success', summary: '已复制', life: 1500 });
    } catch (e) {
        console.error(e);
    }
};
</script>

<template>
    <div class="grid grid-cols-1 gap-4">
        <Card class="w-full">
            <template #title>中心连接信息</template>
            <template #content>
                <table class="w-full text-sm">
                    <thead>
                        <tr class="text-left">
                            <th class="py-1 pr-3">端口</th>
                            <th class="py-1 pr-3">协议</th>
                            <th class="py-1 pr-3">服务</th>
                            <th class="py-1 pr-3">客户端填写</th>
                            <th></th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr v-if="centerInfo">
                            <td class="py-1 pr-3">{{ centerInfo.api_server_port }}</td>
                            <td class="py-1 pr-3">TCP</td>
                            <td class="py-1 pr-3">Web 控制台 / REST API</td>
                            <td class="py-1 pr-3">—</td>
                            <td></td>
                        </tr>
                        <tr v-if="centerInfo">
                            <td class="py-1 pr-3">{{ centerInfo.config_server_port }}</td>
                            <td class="py-1 pr-3">{{ centerInfo.config_server_protocol.toUpperCase() }}</td>
                            <td class="py-1 pr-3">config-server（注册 / 配置下发）</td>
                            <td class="py-1 pr-3"><code>{{ configServerAddress }}</code></td>
                            <td>
                                <Button size="small" label="复制" @click="copy(configServerAddress)" />
                            </td>
                        </tr>
                        <tr v-if="centerInfo?.anf_center_peer_url">
                            <td class="py-1 pr-3">{{ peerPort(centerInfo.anf_center_peer_url) }}</td>
                            <td class="py-1 pr-3">TCP+UDP</td>
                            <td class="py-1 pr-3">中心 core（中继 / 兜底）</td>
                            <td class="py-1 pr-3"><code>{{ centerInfo.anf_center_peer_url }}</code></td>
                            <td>
                                <Button size="small" label="复制"
                                    @click="copy(centerInfo?.anf_center_peer_url ?? '')" />
                            </td>
                        </tr>
                    </tbody>
                </table>
                <p class="text-xs text-gray-500 mt-2">
                    端口以本实例运行时配置为准；网络名 {{ centerInfo?.anf_network_name ?? '—' }}，版本
                    {{ centerInfo?.version ?? '—' }}
                </p>
            </template>
        </Card>

        <Card v-if="stats" class="w-full">
            <template #title>ANF 概览</template>
            <template #content>
                <div class="grid grid-cols-3 gap-2 text-center sm:grid-cols-4">
                    <div>总设备<br /><b>{{ stats.total_devices }}</b></div>
                    <div>待审批<br /><b>{{ stats.pending }}</b></div>
                    <div>已放行<br /><b>{{ stats.approved }}</b></div>
                    <div>已拒绝<br /><b>{{ stats.rejected }}</b></div>
                    <div>已踢出<br /><b>{{ stats.kicked }}</b></div>
                    <div>网络<br /><b>{{ stats.networks }}</b></div>
                    <div>Tag<br /><b>{{ stats.tags }}</b></div>
                    <div>ACL 规则<br /><b>{{ stats.rules }}</b></div>
                </div>
            </template>
        </Card>
    </div>
</template>
