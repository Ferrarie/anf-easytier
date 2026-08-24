export interface VersionDisplay {
  gui: string
  core: string
}

export function formatVersionDisplay(guiVersion: string, coreVersion: string): VersionDisplay {
  return {
    gui: `v${guiVersion}`,
    core: coreVersion,
  }
}
