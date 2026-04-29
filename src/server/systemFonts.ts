import { readdir, readFile } from "node:fs/promises";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import type { SystemFont } from "../shared/types";

const FONT_EXTENSIONS = new Set([".otf", ".ttc", ".ttf", ".woff", ".woff2"]);
const MAX_FONT_FILES = 5000;
const MAX_FONT_BYTES = 8 * 1024 * 1024;

let cachedFonts: SystemFont[] | null = null;

type FontNameRecord = {
  family?: string;
  fullName?: string;
  postScriptName?: string;
};

export async function listSystemFonts(): Promise<SystemFont[]> {
  if (cachedFonts) {
    return cachedFonts;
  }

  const fontPaths = await collectFontPaths(resolveFontDirectories());
  const fonts = new Map<string, SystemFont>();

  for (const fontPath of fontPaths) {
    try {
      const records = parseFontNameRecords(await readFile(fontPath));
      for (const record of records) {
        const family = cleanFontName(record.family || record.fullName);
        if (!family) {
          continue;
        }
        const key = family.toLocaleLowerCase();
        if (!fonts.has(key)) {
          fonts.set(key, {
            family,
            fullName: cleanFontName(record.fullName),
            postScriptName: cleanFontName(record.postScriptName),
            cssFamily: `${quoteCssFontFamily(family)}, "Malgun Gothic", "Apple SD Gothic Neo", sans-serif`
          });
        }
      }
    } catch {
      // Protected or unsupported font files should not block the editor.
    }
  }

  cachedFonts = [...fonts.values()].sort((a, b) => a.family.localeCompare(b.family, undefined, { sensitivity: "base" }));
  return cachedFonts;
}

function resolveFontDirectories(): string[] {
  const currentPlatform = platform();
  if (currentPlatform === "darwin") {
    return [
      join(homedir(), "Library", "Fonts"),
      "/Library/Fonts",
      "/System/Library/Fonts",
      "/System/Library/AssetsV2/com_apple_MobileAsset_Font7"
    ];
  }
  if (currentPlatform === "win32") {
    return [
      join(process.env.WINDIR || "C:\\Windows", "Fonts"),
      join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Microsoft", "Windows", "Fonts")
    ];
  }
  return [join(homedir(), ".fonts"), join(homedir(), ".local", "share", "fonts"), "/usr/local/share/fonts", "/usr/share/fonts"];
}

async function collectFontPaths(rootDirs: string[]): Promise<string[]> {
  const paths: string[] = [];
  const visited = new Set<string>();
  const queue = [...new Set(rootDirs)];

  while (queue.length > 0 && paths.length < MAX_FONT_FILES) {
    const dir = queue.shift()!;
    if (visited.has(dir)) {
      continue;
    }
    visited.add(dir);

    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      continue;
    }

    for (const entry of entries) {
      const fullPath = join(dir, entry.name);
      if (entry.isDirectory()) {
        queue.push(fullPath);
        continue;
      }
      if (!entry.isFile()) {
        continue;
      }
      const dotIndex = entry.name.lastIndexOf(".");
      const extension = dotIndex >= 0 ? entry.name.slice(dotIndex).toLowerCase() : "";
      if (FONT_EXTENSIONS.has(extension)) {
        paths.push(fullPath);
        if (paths.length >= MAX_FONT_FILES) {
          break;
        }
      }
    }
  }

  return paths;
}

function parseFontNameRecords(buffer: Buffer): FontNameRecord[] {
  if (buffer.length > MAX_FONT_BYTES) {
    return [];
  }
  if (buffer.subarray(0, 4).toString("ascii") === "ttcf") {
    const fontCount = buffer.readUInt32BE(8);
    const records: FontNameRecord[] = [];
    for (let index = 0; index < fontCount; index += 1) {
      const offsetPosition = 12 + index * 4;
      if (offsetPosition + 4 > buffer.length) {
        break;
      }
      records.push(...parseSfntNameRecords(buffer, buffer.readUInt32BE(offsetPosition)));
    }
    return records;
  }
  return parseSfntNameRecords(buffer, 0);
}

function parseSfntNameRecords(buffer: Buffer, sfntOffset: number): FontNameRecord[] {
  if (sfntOffset < 0 || sfntOffset + 12 > buffer.length) {
    return [];
  }

  const numTables = buffer.readUInt16BE(sfntOffset + 4);
  let nameTableOffset = -1;
  let nameTableLength = 0;

  for (let index = 0; index < numTables; index += 1) {
    const entryOffset = sfntOffset + 12 + index * 16;
    if (entryOffset + 16 > buffer.length) {
      return [];
    }
    if (buffer.subarray(entryOffset, entryOffset + 4).toString("ascii") === "name") {
      nameTableOffset = buffer.readUInt32BE(entryOffset + 8);
      nameTableLength = buffer.readUInt32BE(entryOffset + 12);
      break;
    }
  }

  if (nameTableOffset < 0 || nameTableOffset + 6 > buffer.length || nameTableOffset + nameTableLength > buffer.length) {
    return [];
  }

  const count = buffer.readUInt16BE(nameTableOffset + 2);
  const stringStorageOffset = nameTableOffset + buffer.readUInt16BE(nameTableOffset + 4);
  const best: Record<"family" | "fullName" | "postScriptName", { score: number; value: string } | undefined> = {
    family: undefined,
    fullName: undefined,
    postScriptName: undefined
  };

  for (let index = 0; index < count; index += 1) {
    const recordOffset = nameTableOffset + 6 + index * 12;
    if (recordOffset + 12 > buffer.length) {
      break;
    }

    const platformId = buffer.readUInt16BE(recordOffset);
    const encodingId = buffer.readUInt16BE(recordOffset + 2);
    const languageId = buffer.readUInt16BE(recordOffset + 4);
    const nameId = buffer.readUInt16BE(recordOffset + 6);
    const length = buffer.readUInt16BE(recordOffset + 8);
    const stringOffset = stringStorageOffset + buffer.readUInt16BE(recordOffset + 10);
    if (stringOffset + length > buffer.length) {
      continue;
    }

    const field = nameId === 16 || nameId === 1 ? "family" : nameId === 4 ? "fullName" : nameId === 6 ? "postScriptName" : null;
    if (!field) {
      continue;
    }

    const value = cleanFontName(decodeNameString(buffer.subarray(stringOffset, stringOffset + length), platformId, encodingId));
    if (!value) {
      continue;
    }

    const score = scoreNameRecord(platformId, languageId, nameId);
    if (!best[field] || score > best[field].score) {
      best[field] = { score, value };
    }
  }

  return [{ family: best.family?.value, fullName: best.fullName?.value, postScriptName: best.postScriptName?.value }];
}

function decodeNameString(bytes: Buffer, platformId: number, encodingId: number): string {
  if (platformId === 0 || platformId === 3 || (platformId === 2 && encodingId === 1)) {
    return Buffer.from(bytes).swap16().toString("utf16le");
  }
  if (platformId === 1) {
    const encoding = resolveMacEncoding(encodingId);
    if (encoding) {
      try {
        return new TextDecoder(encoding).decode(bytes);
      } catch {
        return bytes.toString("latin1");
      }
    }
  }
  return bytes.toString("latin1");
}

function resolveMacEncoding(encodingId: number): string | null {
  if (encodingId === 0) {
    return "macintosh";
  }
  if (encodingId === 1) {
    return "shift_jis";
  }
  if (encodingId === 2) {
    return "big5";
  }
  if (encodingId === 3) {
    return "euc-kr";
  }
  if (encodingId === 25) {
    return "gbk";
  }
  return null;
}

function scoreNameRecord(platformId: number, languageId: number, nameId: number): number {
  let score = 0;
  if (nameId === 16) {
    score += 20;
  }
  if (platformId === 3) {
    score += 10;
  }
  if (languageId === 0x0412 || languageId === 0x0409 || languageId === 0) {
    score += 5;
  }
  return score;
}

function cleanFontName(value: string | undefined): string | undefined {
  const cleaned = value?.replace(/\0/g, "").replace(/\s+/g, " ").trim();
  if (!cleaned || cleaned.startsWith(".")) {
    return undefined;
  }
  return cleaned;
}

function quoteCssFontFamily(family: string): string {
  return `"${family.replace(/\\/g, "\\\\").replace(/"/g, "\\\"")}"`;
}
