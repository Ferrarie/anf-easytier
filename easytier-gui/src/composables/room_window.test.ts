import { describe, expect, it } from 'vitest'
import { canOpenMemberWindow, resolveMemberWindowAction } from './room_window'

describe('resolveMemberWindowAction', () => {
  it('无窗口 -> create', () => {
    expect(resolveMemberWindowAction(null)).toBe('create')
  })
  it('窗口可见 -> close', () => {
    expect(resolveMemberWindowAction({ visible: true })).toBe('close')
  })
  it('窗口隐藏 -> show', () => {
    expect(resolveMemberWindowAction({ visible: false })).toBe('show')
  })
})

describe('canOpenMemberWindow', () => {
  it('仅客户端运行/连接/待审且存在实例时可开', () => {
    expect(canOpenMemberWindow('connected', 'inst-1')).toBe(true)
    expect(canOpenMemberWindow('pending', 'inst-1')).toBe(true)
    expect(canOpenMemberWindow('connecting', 'inst-1')).toBe(true)
  })
  it('失败/未连接或无实例时不可开', () => {
    expect(canOpenMemberWindow('failed', 'inst-1')).toBe(false)
    expect(canOpenMemberWindow('connected', undefined)).toBe(false)
    expect(canOpenMemberWindow('', 'inst-1')).toBe(false)
  })
})
