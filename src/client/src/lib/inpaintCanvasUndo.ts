export function createUndoCanvasSnapshot(canvas: HTMLCanvasElement, previousDataUrl: string | undefined): HTMLCanvasElement | null {
  if (!previousDataUrl || isInlineDataUrl(previousDataUrl)) {
    return null;
  }
  const snapshot = document.createElement("canvas");
  snapshot.width = canvas.width;
  snapshot.height = canvas.height;
  const context = snapshot.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return null;
  }
  context.drawImage(canvas, 0, 0);
  return snapshot;
}

export function isInlineDataUrl(value: string | undefined): boolean {
  return Boolean(value?.startsWith("data:"));
}
