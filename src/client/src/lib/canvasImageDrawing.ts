export type CanvasImageDrawSize = {
  width: number;
  height: number;
};

export function resizeCanvasToSize(canvas: HTMLCanvasElement, size: CanvasImageDrawSize): void {
  ensureCanvasSize(canvas, size);
}

export function clearCanvasToSize(
  canvas: HTMLCanvasElement,
  context: CanvasRenderingContext2D,
  size: CanvasImageDrawSize
): void {
  ensureCanvasSize(canvas, size);
  context.clearRect(0, 0, canvas.width, canvas.height);
}

export function drawImageToCanvas(
  canvas: HTMLCanvasElement,
  context: CanvasRenderingContext2D,
  image: CanvasImageSource,
  size: CanvasImageDrawSize
): void {
  ensureCanvasSize(canvas, size);
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
}

export function loadCanvasImage(src: string, message: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(message));
    image.src = src;
  });
}

function ensureCanvasSize(canvas: HTMLCanvasElement, size: CanvasImageDrawSize): void {
  if (canvas.width !== size.width) {
    canvas.width = size.width;
  }
  if (canvas.height !== size.height) {
    canvas.height = size.height;
  }
}
