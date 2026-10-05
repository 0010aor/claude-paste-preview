export const LABEL_GAP = 2

export function labelText(number: number) {
  return `Image #${number}`
}

export function labelIndexAt(column: number, numbers: readonly number[]): number | undefined {
  let start = 0
  for (const [index, number] of numbers.entries()) {
    const end = start + labelText(number).length + LABEL_GAP
    if (column >= start && column < end) return index
    start = end
  }
  return undefined
}
