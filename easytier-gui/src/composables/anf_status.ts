import type { AnfStatus } from './anf_first_screen'

export interface AnfStatusMeta {
  label: string
  tone: 'neutral' | 'accent' | 'warn' | 'success' | 'danger'
  icon: string
  pulse?: boolean
}

const META: Record<AnfStatus, AnfStatusMeta> = {
  idle: { label: '未连接', tone: 'neutral', icon: 'pi pi-power-off' },
  connecting: { label: '连接中…', tone: 'accent', icon: 'pi pi-spin pi-spinner', pulse: true },
  pending: { label: '等待审批', tone: 'warn', icon: 'pi pi-clock' },
  connected: { label: '已连接', tone: 'success', icon: 'pi pi-check-circle' },
  failed: { label: '连接失败', tone: 'danger', icon: 'pi pi-exclamation-circle' },
}

export function anfStatusMeta(status: AnfStatus): AnfStatusMeta {
  return META[status] ?? META.idle
}
