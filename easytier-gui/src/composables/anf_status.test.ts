import { describe, expect, it } from 'vitest'

import { anfStatusMeta } from './anf_status'

describe('anfStatusMeta', () => {
  it('idle -> 未连接 neutral', () => {
    expect(anfStatusMeta('idle')).toMatchObject({ label: '未连接', tone: 'neutral' })
  })

  it('connecting -> 连接中 accent + 旋转图标', () => {
    expect(anfStatusMeta('connecting')).toMatchObject({ label: '连接中…', tone: 'accent', pulse: true })
  })

  it('pending -> 等待审批 warn', () => {
    expect(anfStatusMeta('pending')).toMatchObject({ label: '等待审批', tone: 'warn' })
  })

  it('connected -> 已连接 success', () => {
    expect(anfStatusMeta('connected')).toMatchObject({ label: '已连接', tone: 'success' })
  })

  it('failed -> 连接失败 danger', () => {
    expect(anfStatusMeta('failed')).toMatchObject({ label: '连接失败', tone: 'danger' })
  })

  it('未知状态回退 idle', () => {
    expect(anfStatusMeta('whatever' as never)).toMatchObject({ label: '未连接', tone: 'neutral' })
  })
})
