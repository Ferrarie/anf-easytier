/**
 * 把多选组件（MultiSelect/Dropdown）回填的任意值归一化为干净的 string[]。
 *
 * 覆盖三种形态：
 * 1. 已经是 string[] → 原样返回（会过滤空串）；
 * 2. 数组内是 { label, value } / { name } 对象 → 取 value/name；其他对象 → String(x)；
 * 3. 标量/逗号字符串 → 按逗号切分（兼容旧版文本框输入）。
 *
 * 这保证 PATCH /api/v1/devices/:id 的 tags/networks 始终是 Vec<String>，避免 422。
 */
export function toStringArray(v: unknown): string[] {
    if (!Array.isArray(v)) {
        return v ? String(v).split(',').map((s) => s.trim()).filter(Boolean) : [];
    }
    return v
        .filter((x: any) => x != null)
        .map((x: any) => (typeof x === 'string' ? x : (x?.value ?? x?.name ?? String(x))))
        .map((s: string) => s.trim())
        .filter((s: string) => s.length > 0);
}
