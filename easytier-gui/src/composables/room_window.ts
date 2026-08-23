// 房间信息（成员列表）独立窗口：开合决策（纯函数）+ Tauri 窗口封装。

export type MemberWindowAction = 'create' | 'close' | 'show'

/**
 * 根据已存在的 member 窗口状态决定动作：无窗口->create；可见->close（再点关闭）；
 * 隐藏->show。参照 easytier-game `composables/windows.ts` 的 etWindows。
 */
export function resolveMemberWindowAction(existing: { visible: boolean } | null): MemberWindowAction {
  if (!existing) return 'create'
  return existing.visible ? 'close' : 'show'
}

// 客户端已启动/连接/待审时，成员实例信息才可用（否则按钮置灰）。
const OPENABLE = ['connecting', 'pending', 'connected']

export function canOpenMemberWindow(status: string, lastInstanceId?: string): boolean {
  return OPENABLE.includes(status) && !!lastInstanceId
}

/**
 * 开合 member 窗口。动态 import Tauri API，避免在 vitest 单测中加载 Tauri 运行时。
 */
export async function toggleMemberWindow(status: string, lastInstanceId?: string): Promise<void> {
  if (!canOpenMemberWindow(status, lastInstanceId)) return

  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  const { getCurrentWindow, PhysicalPosition } = await import('@tauri-apps/api/window')

  const existing = await WebviewWindow.getByLabel('member')
  const action = resolveMemberWindowAction(existing ? { visible: await existing.isVisible() } : null)

  if (action === 'close') {
    await existing?.close()
    return
  }
  if (action === 'show') {
    await existing?.show()
    await existing?.setFocus()
    return
  }

  const app = getCurrentWindow()
  const factor = await app.scaleFactor()
  const pos = await app.outerPosition()
  const logical = new PhysicalPosition(pos.x + Math.ceil(345 * factor), pos.y).toLogical(factor)

  new WebviewWindow('member', {
    title: '成员列表',
    width: 880,
    height: 460,
    url: '#/member',
    parent: app,
    x: logical.x,
    y: logical.y,
    closable: true,
    resizable: true,
    decorations: true,
    maximizable: false,
    minimizable: false,
  })
}
