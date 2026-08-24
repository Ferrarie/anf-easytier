import { describe, expect, it } from 'vitest'

import { formatVersionDisplay } from './version'

describe('formatVersionDisplay', () => {
  it('GUI 版本带 v 前缀，core 版本保持原样', () => {
    expect(formatVersionDisplay('1.0.0', '2.6.4')).toEqual({ gui: 'v1.0.0', core: '2.6.4' })
  })
})
