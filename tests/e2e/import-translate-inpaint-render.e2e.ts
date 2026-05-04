import { expect, test, type Locator, type Page } from "@playwright/test";
import { _electron as electron, type ElectronApplication } from "playwright";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { mkdir, mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import net from "node:net";
import sharp from "sharp";

const repoRoot = resolve(__dirname, "../..");
const electronExecutable = require("electron") as string;

type MockModelServer = {
  baseUrl: string;
  chatRequests: unknown[];
  close: () => Promise<void>;
};

test.describe("Electron user flow", () => {
  test("imports an image, translates, edits a box, inpaints, and saves a render", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "manga-e2e-"));
    const mockModel = await startMockModelServer();
    const appPort = await getFreePort();
    let app: ElectronApplication | null = null;
    const pageErrors: string[] = [];

    try {
      const imagePath = join(dataRoot, "fixtures", "page-001.png");
      await createFixtureImage(imagePath);
      const electronEnv: Record<string, string> = {
        ...stringProcessEnv(),
        ELECTRON_DISABLE_SECURITY_WARNINGS: "true",
        MANGA_TRANSLATOR_APP_ROOT: repoRoot,
        MANGA_TRANSLATOR_DATA_DIR: dataRoot,
        MANGA_TRANSLATOR_MODEL_PROVIDER: "openai-compatible",
        MANGA_TRANSLATOR_OPENAI_COMPATIBLE_API_KEY: "e2e-key",
        MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL: mockModel.baseUrl,
        MANGA_TRANSLATOR_OPENAI_COMPATIBLE_MODEL: "e2e-overlay-model",
        MANGA_TRANSLATOR_PAGE_RETRIES: "1",
        MANGA_TRANSLATOR_PORT: String(appPort)
      };
      delete electronEnv.ELECTRON_RUN_AS_NODE;

      app = await electron.launch({
        executablePath: electronExecutable,
        args: [join(repoRoot, "electron")],
        env: electronEnv
      });

      const page = await app.firstWindow();
      page.on("pageerror", (error) => pageErrors.push(error.message));
      await expect(page.getByTestId("image-import-input")).toBeAttached();

      await page.getByTestId("image-import-input").setInputFiles(imagePath);
      await expect(page.getByRole("heading", { name: "보관함에 추가" })).toBeVisible();
      await page.getByTestId("import-work-title-input").fill("E2E Flow Work");
      await page.getByTestId("import-chapter-title-input").fill("Chapter 001");
      await page.getByTestId("import-submit-button").click();

      await expect(page.getByTestId("image-stage")).toBeVisible();
      await expect(page.getByRole("button", { name: "계속 번역 (AI)" })).toBeEnabled();

      await page.getByRole("button", { name: "계속 번역 (AI)" }).click();
      await expect(page.getByTestId("translation-block")).toHaveCount(1, { timeout: 60_000 });
      await expect.poll(() => mockModel.chatRequests.length, { timeout: 10_000 }).toBe(1);

      const overlayLayer = page.locator('[data-layer-label="2 번역 블록"]');
      await overlayLayer.locator(".layer-label-text").click();
      await expect(overlayLayer).toHaveAttribute("data-active", "true");
      const block = page.getByTestId("translation-block").first();
      await block.click();
      await expect(page.locator('[data-block-text-field="translated"]')).toHaveValue("테스트 번역");

      const initialBlockBox = await block.boundingBox();
      expect(initialBlockBox).not.toBeNull();
      if (!initialBlockBox) {
        throw new Error("Translation block was not measurable.");
      }

      await dragByMouse(page, block, 24, 16);
      await expect.poll(async () => (await block.boundingBox())?.x ?? initialBlockBox.x, { timeout: 10_000 }).toBeGreaterThan(initialBlockBox.x);
      await page.locator('[data-block-text-field="translated"]').fill("수정된 번역");

      await page.getByRole("button", { name: "인페인트 실행" }).click();
      await expect.poll(async () => findFiles(dataRoot, (filePath) => isPathSegment(filePath, "inpaint") && filePath.endsWith("-result.png")), {
        timeout: 60_000
      }).toHaveLength(1);

      await page.getByRole("button", { name: "페이지 출력", exact: true }).click();
      await expect.poll(async () => findFiles(dataRoot, (filePath) => isPathSegment(filePath, "renders") && filePath.endsWith(".png")), {
        timeout: 60_000
      }).toHaveLength(1);

      const chapter = await readOnlyChapter(dataRoot);
      const savedPage = chapter.pages[0];
      expect(savedPage.blocks[0].translatedText).toBe("수정된 번역");
      expect(savedPage.blocks[0].bbox.x).toBeGreaterThan(170);
      expect(savedPage.inpaintMaskPath).toBeTruthy();
      expect(savedPage.inpaintResultPath).toBeTruthy();
      expect(pageErrors).toEqual([]);
    } finally {
      await app?.close().catch(() => undefined);
      await mockModel.close();
      await rm(dataRoot, { recursive: true, force: true });
    }
  });
});

async function dragByMouse(page: Page, locator: Locator, dx: number, dy: number): Promise<void> {
  const box = await locator.boundingBox();
  if (!box) {
    throw new Error("Translation block was not measurable for dragging.");
  }
  const startX = box.x + box.width / 2;
  const startY = box.y + box.height / 2;

  await page.mouse.move(startX, startY);
  await page.mouse.down();
  for (const step of [0.25, 0.5, 0.75, 1]) {
    await page.mouse.move(startX + dx * step, startY + dy * step);
  }
  await page.mouse.up();

  const movedBox = await locator.boundingBox();
  if (movedBox && (Math.abs(movedBox.x - box.x) > 0.5 || Math.abs(movedBox.y - box.y) > 0.5)) {
    return;
  }

  await dragByPointerEvents(locator, startX, startY, dx, dy);
}

async function dragByPointerEvents(locator: Locator, startX: number, startY: number, dx: number, dy: number): Promise<void> {
  const pointerId = 1;
  await locator.dispatchEvent("pointerdown", {
    pointerId,
    pointerType: "mouse",
    isPrimary: true,
    clientX: startX,
    clientY: startY,
    button: 0,
    buttons: 1
  });
  for (const step of [0.25, 0.5, 0.75, 1]) {
    await locator.dispatchEvent("pointermove", {
      pointerId,
      pointerType: "mouse",
      isPrimary: true,
      clientX: startX + dx * step,
      clientY: startY + dy * step,
      button: 0,
      buttons: 1
    });
  }
  await locator.dispatchEvent("pointerup", {
    pointerId,
    pointerType: "mouse",
    isPrimary: true,
    clientX: startX + dx,
    clientY: startY + dy,
    button: 0,
    buttons: 0
  });
}

async function startMockModelServer(): Promise<MockModelServer> {
  const chatRequests: unknown[] = [];
  const server = createServer(async (request, response) => {
    try {
      if (request.method === "GET" && request.url === "/v1/models") {
        sendJson(response, {
          object: "list",
          data: [{ id: "e2e-overlay-model", object: "model" }]
        });
        return;
      }

      if (request.method === "POST" && request.url === "/v1/chat/completions") {
        const requestBody = await readJsonBody(request);
        chatRequests.push(requestBody);
        sendJson(response, {
          id: "chatcmpl-e2e",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              finish_reason: "stop",
              message: {
                role: "assistant",
                content: JSON.stringify({
                  items: [
                    {
                      id: 1,
                      type: "speech",
                      bbox: { x: 170, y: 180, w: 360, h: 170 },
                      jp: "テスト",
                      ko: "테스트 번역"
                    }
                  ]
                })
              }
            }
          ]
        });
        return;
      }

      response.writeHead(404, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ error: `Unhandled mock route ${request.method} ${request.url}` }));
    } catch (error) {
      response.writeHead(500, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ error: error instanceof Error ? error.message : String(error) }));
    }
  });

  await listen(server, 0);
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("Mock model server did not bind to a TCP port.");
  }

  return {
    baseUrl: `http://127.0.0.1:${address.port}/v1`,
    chatRequests,
    close: () => close(server)
  };
}

async function createFixtureImage(outputPath: string): Promise<void> {
  await mkdir(dirname(outputPath), { recursive: true });
  const svg = `
    <svg width="320" height="420" viewBox="0 0 320 420" xmlns="http://www.w3.org/2000/svg">
      <rect width="320" height="420" fill="#f8f5ee"/>
      <rect x="54" y="76" width="116" height="70" rx="8" fill="#fffdf5" stroke="#151515" stroke-width="4"/>
      <rect x="42" y="198" width="238" height="166" rx="18" fill="#d8d4c8" stroke="#202020" stroke-width="5"/>
      <text x="112" y="120" text-anchor="middle" font-family="Arial" font-size="26" font-weight="700" fill="#111111">JP</text>
    </svg>
  `;
  await sharp(Buffer.from(svg)).png().toFile(outputPath);
}

async function getFreePort(): Promise<number> {
  const server = net.createServer();
  await new Promise<void>((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  await close(server);
  if (!address || typeof address === "string") {
    throw new Error("Failed to allocate a free TCP port.");
  }
  return address.port;
}

function listen(server: Server, port: number): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(port, "127.0.0.1", resolveListen);
  });
}

function close(server: Server | net.Server): Promise<void> {
  return new Promise((resolveClose) => {
    server.close(() => resolveClose());
  });
}

function sendJson(response: ServerResponse, payload: unknown): void {
  response.writeHead(200, { "Content-Type": "application/json" });
  response.end(JSON.stringify(payload));
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

async function findFiles(root: string, predicate: (filePath: string) => boolean): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = join(root, entry.name);
      if (entry.isDirectory()) {
        return findFiles(path, predicate);
      }
      return predicate(path) ? [path] : [];
    })
  );
  return files.flat();
}

async function readOnlyChapter(dataRoot: string): Promise<{ pages: Array<{
  blocks: Array<{ bbox: { x: number }; translatedText: string }>;
  inpaintMaskPath?: string;
  inpaintResultPath?: string;
}> }> {
  const [chapterPath] = await findFiles(dataRoot, (filePath) => filePath.endsWith(`${sep}chapter.json`));
  if (!chapterPath) {
    throw new Error("No chapter.json was saved.");
  }
  return JSON.parse(await readFile(chapterPath, "utf8"));
}

function isPathSegment(filePath: string, segment: string): boolean {
  return filePath.includes(`${sep}${segment}${sep}`);
}

function stringProcessEnv(): Record<string, string> {
  return Object.fromEntries(Object.entries(process.env).filter((entry): entry is [string, string] => typeof entry[1] === "string"));
}
