import { readdir, readFile } from "node:fs/promises";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import type { SystemFont } from "../shared/types";

const FONT_EXTENSIONS = new Set([".otf", ".ttc", ".ttf", ".woff", ".woff2"]);
const MAX_FONT_FILES = 5000;
const MAX_FONT_BYTES = 32 * 1024 * 1024;

let cachedFonts: SystemFont[] | null = null;

type FontNameRecord = {
  family?: string;
  fullName?: string;
  postScriptName?: string;
  subfamily?: string;
  weights?: number[];
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
        const family = normalizeFontFamilyName(record);
        if (!family) {
          continue;
        }
        const key = family.toLocaleLowerCase();
        const weights = record.weights?.length ? record.weights : [inferFontWeight(record)].filter((weight): weight is number => Boolean(weight));
        const existing = fonts.get(key);
        if (existing) {
          existing.weights = mergeFontWeights(existing.weights, weights);
        } else {
          fonts.set(key, {
            family,
            fullName: cleanFontName(record.fullName),
            postScriptName: cleanFontName(record.postScriptName),
            weights: weights.length > 0 ? weights : undefined,
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
  let os2TableOffset = -1;
  let fvarTableOffset = -1;
  let fvarTableLength = 0;

  for (let index = 0; index < numTables; index += 1) {
    const entryOffset = sfntOffset + 12 + index * 16;
    if (entryOffset + 16 > buffer.length) {
      return [];
    }
    const tag = buffer.subarray(entryOffset, entryOffset + 4).toString("ascii");
    if (tag === "name") {
      nameTableOffset = buffer.readUInt32BE(entryOffset + 8);
      nameTableLength = buffer.readUInt32BE(entryOffset + 12);
    } else if (tag === "OS/2") {
      os2TableOffset = buffer.readUInt32BE(entryOffset + 8);
    } else if (tag === "fvar") {
      fvarTableOffset = buffer.readUInt32BE(entryOffset + 8);
      fvarTableLength = buffer.readUInt32BE(entryOffset + 12);
    }
  }

  if (nameTableOffset < 0 || nameTableOffset + 6 > buffer.length || nameTableOffset + nameTableLength > buffer.length) {
    return [];
  }

  const staticWeight = parseOs2Weight(buffer, os2TableOffset);
  const variableWeights = parseFvarWeights(buffer, fvarTableOffset, fvarTableLength);
  const count = buffer.readUInt16BE(nameTableOffset + 2);
  const stringStorageOffset = nameTableOffset + buffer.readUInt16BE(nameTableOffset + 4);
  const best: Record<"family" | "fullName" | "postScriptName" | "subfamily", { score: number; value: string } | undefined> = {
    family: undefined,
    fullName: undefined,
    postScriptName: undefined,
    subfamily: undefined
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

    const field =
      nameId === 16 || nameId === 1
        ? "family"
        : nameId === 4
          ? "fullName"
          : nameId === 6
            ? "postScriptName"
            : nameId === 17 || nameId === 2
              ? "subfamily"
              : null;
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

  return [{
    family: best.family?.value,
    fullName: best.fullName?.value,
    postScriptName: best.postScriptName?.value,
    subfamily: best.subfamily?.value,
    weights: variableWeights.length > 0 ? variableWeights : staticWeight ? [staticWeight] : undefined
  }];
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

function mergeFontWeights(current: number[] | undefined, weights: number[]): number[] | undefined {
  if (weights.length === 0) {
    return current;
  }
  return [...new Set([...(current ?? []), ...weights])].sort((a, b) => a - b);
}

function normalizeFontFamilyName(record: FontNameRecord): string | undefined {
  const family = cleanFontName(record.family || record.fullName);
  if (!family) {
    return undefined;
  }
  const inferredWeight = inferFontWeight(record);
  if (!inferredWeight) {
    return family;
  }
  return stripMatchingFontWeightSuffix(family, inferredWeight) ?? family;
}

function inferFontWeight(record: FontNameRecord): number | undefined {
  const name = `${record.subfamily ?? ""} ${record.fullName ?? ""} ${record.postScriptName ?? ""}`.toLocaleLowerCase();
  if (!name.trim()) {
    return undefined;
  }
  if (/\b(?:thin|hairline)\b/.test(name)) {
    return 100;
  }
  if (/\b(?:extra\s*light|ultra\s*light|extralight|ultralight)\b/.test(name)) {
    return 200;
  }
  if (/\b(?:light)\b/.test(name)) {
    return 300;
  }
  if (/\b(?:regular|normal|roman|book)\b/.test(name)) {
    return 400;
  }
  if (/\b(?:medium)\b/.test(name)) {
    return 500;
  }
  if (/\b(?:semi\s*bold|demi\s*bold|semibold|demibold)\b/.test(name)) {
    return 600;
  }
  if (/\b(?:extra\s*bold|ultra\s*bold|extrabold|ultrabold)\b/.test(name)) {
    return 800;
  }
  if (/\b(?:bold)\b/.test(name)) {
    return 700;
  }
  if (/\b(?:black|heavy)\b/.test(name)) {
    return 900;
  }
  return undefined;
}

function stripMatchingFontWeightSuffix(family: string, weight: number): string | undefined {
  const suffixPattern = FONT_WEIGHT_SUFFIX_PATTERNS[weight];
  if (!suffixPattern) {
    return undefined;
  }

  const normalized = cleanFontName(family.replace(suffixPattern, ""));
  return normalized && normalized !== family ? normalized : undefined;
}

const FONT_WEIGHT_SUFFIX_PATTERNS: Record<number, RegExp> = {
  100: /\s+(?:Thin|Hairline)$/i,
  200: /\s+(?:Extra\s*Light|Ultra\s*Light|ExtraLight|UltraLight)$/i,
  300: /\s+Light$/i,
  400: /\s+(?:Regular|Normal|Roman|Book)$/i,
  500: /\s+Medium$/i,
  600: /\s+(?:Semi\s*Bold|Demi\s*Bold|SemiBold|DemiBold)$/i,
  700: /\s+Bold$/i,
  800: /\s+(?:Extra\s*Bold|Ultra\s*Bold|ExtraBold|UltraBold)$/i,
  900: /\s+(?:Black|Heavy)$/i
};

function parseOs2Weight(buffer: Buffer, os2TableOffset: number): number | undefined {
  if (os2TableOffset < 0 || os2TableOffset + 6 > buffer.length) {
    return undefined;
  }
  return normalizeFontWeight(buffer.readUInt16BE(os2TableOffset + 4));
}

function parseFvarWeights(buffer: Buffer, fvarTableOffset: number, fvarTableLength: number): number[] {
  if (fvarTableOffset < 0 || fvarTableLength <= 0 || fvarTableOffset + 16 > buffer.length || fvarTableOffset + fvarTableLength > buffer.length) {
    return [];
  }

  const axesArrayOffset = buffer.readUInt16BE(fvarTableOffset + 4);
  const axisCount = buffer.readUInt16BE(fvarTableOffset + 8);
  const axisSize = buffer.readUInt16BE(fvarTableOffset + 10);
  const instanceCount = buffer.readUInt16BE(fvarTableOffset + 12);
  const instanceSize = buffer.readUInt16BE(fvarTableOffset + 14);
  const axesOffset = fvarTableOffset + axesArrayOffset;
  let weightAxisIndex = -1;
  let minWeight: number | undefined;
  let maxWeight: number | undefined;

  for (let index = 0; index < axisCount; index += 1) {
    const axisOffset = axesOffset + index * axisSize;
    if (axisOffset + 16 > buffer.length) {
      break;
    }
    if (buffer.subarray(axisOffset, axisOffset + 4).toString("ascii") !== "wght") {
      continue;
    }
    weightAxisIndex = index;
    minWeight = readFixed1616(buffer, axisOffset + 4);
    maxWeight = readFixed1616(buffer, axisOffset + 12);
  }

  if (weightAxisIndex < 0) {
    return [];
  }

  const weights: number[] = [];
  const instancesOffset = axesOffset + axisCount * axisSize;
  for (let index = 0; index < instanceCount; index += 1) {
    const instanceOffset = instancesOffset + index * instanceSize;
    const coordinateOffset = instanceOffset + 4;
    const weightCoordinateOffset = coordinateOffset + weightAxisIndex * 4;
    if (weightCoordinateOffset + 4 > buffer.length) {
      break;
    }
    const weight = normalizeFontWeight(readFixed1616(buffer, weightCoordinateOffset));
    if (weight) {
      weights.push(weight);
    }
  }

  if (weights.length === 0 && minWeight && maxWeight) {
    const minSteppedWeight = Math.ceil(minWeight / 100) * 100;
    const maxSteppedWeight = Math.floor(maxWeight / 100) * 100;
    for (let weight = minSteppedWeight; weight <= maxSteppedWeight; weight += 100) {
      const normalized = normalizeFontWeight(weight);
      if (normalized) {
        weights.push(normalized);
      }
    }
  }

  return [...new Set(weights)].sort((a, b) => a - b);
}

function readFixed1616(buffer: Buffer, offset: number): number {
  return buffer.readInt32BE(offset) / 65536;
}

function normalizeFontWeight(value: number): number | undefined {
  if (!Number.isFinite(value)) {
    return undefined;
  }
  const rounded = roundFontWeight(value);
  if (rounded < 100 || rounded > 900) {
    return undefined;
  }
  return rounded;
}

function roundFontWeight(value: number): number {
  return Math.round(value / 100) * 100;
}

function quoteCssFontFamily(family: string): string {
  return `"${family.replace(/\\/g, "\\\\").replace(/"/g, "\\\"")}"`;
}

export const systemFontsTestHooks = {
  mergeFontWeights,
  normalizeFontFamilyName,
  parseFontNameRecords
};
