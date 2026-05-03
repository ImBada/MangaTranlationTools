import { describe, expect, it } from "vitest";
import { systemFontsTestHooks } from "../src/server/systemFonts";

type NameRecordFixture = {
  nameId: number;
  value: string;
};

type TableFixture = {
  tag: string;
  data: Buffer;
};

const { mergeFontWeights, normalizeFontFamilyName, parseFontNameRecords } = systemFontsTestHooks;

describe("system font OpenType parsing", () => {
  it("reads static font weight from the OS/2 table", () => {
    const records = parseFontNameRecords(buildSfnt([
      buildNameTable([
        { nameId: 16, value: "Pretendard" },
        { nameId: 17, value: "Bold" },
        { nameId: 4, value: "Pretendard Bold" },
        { nameId: 6, value: "Pretendard-Bold" }
      ]),
      buildOs2Table(700)
    ]));

    expect(records).toEqual([{
      family: "Pretendard",
      fullName: "Pretendard Bold",
      postScriptName: "Pretendard-Bold",
      subfamily: "Bold",
      weights: [700]
    }]);
  });

  it("reads named variable font weights from the fvar table", () => {
    const records = parseFontNameRecords(buildSfnt([
      buildNameTable([
        { nameId: 16, value: "Pretendard Variable" },
        { nameId: 17, value: "Regular" },
        { nameId: 4, value: "Pretendard Variable Regular" },
        { nameId: 6, value: "PretendardVariable-Regular" }
      ]),
      buildFvarTable({
        minWeight: 100,
        maxWeight: 900,
        instanceWeights: [100, 400, 700, 900]
      })
    ]));

    expect(records[0]?.weights).toEqual([100, 400, 700, 900]);
  });

  it("falls back to 100-step variable weights when fvar has no named instances", () => {
    const records = parseFontNameRecords(buildSfnt([
      buildNameTable([{ nameId: 16, value: "Range Variable" }]),
      buildFvarTable({
        minWeight: 250,
        maxWeight: 760,
        instanceWeights: []
      })
    ]));

    expect(records[0]?.weights).toEqual([300, 400, 500, 600, 700]);
  });

  it("normalizes matching weight suffixes without stripping unrelated family names", () => {
    expect(normalizeFontFamilyName({
      family: "Pretendard Bold",
      fullName: "Pretendard Bold",
      postScriptName: "Pretendard-Bold",
      subfamily: "Bold"
    })).toBe("Pretendard");

    expect(normalizeFontFamilyName({
      family: "Arial Black",
      fullName: "Arial Black Regular",
      postScriptName: "ArialBlack-Regular",
      subfamily: "Regular"
    })).toBe("Arial Black");
  });

  it("merges detected weights in numeric order", () => {
    expect(mergeFontWeights([700, 400], [100, 700, 900])).toEqual([100, 400, 700, 900]);
    expect(mergeFontWeights(undefined, [])).toBeUndefined();
  });
});

function buildSfnt(tables: TableFixture[]): Buffer {
  const tableCount = tables.length;
  const headerLength = 12 + tableCount * 16;
  let nextTableOffset = headerLength;
  const tableEntries = tables.map((table) => {
    const offset = nextTableOffset;
    nextTableOffset += table.data.length;
    return { ...table, offset };
  });
  const buffer = Buffer.alloc(nextTableOffset);
  buffer.writeUInt32BE(0x00010000, 0);
  buffer.writeUInt16BE(tableCount, 4);

  tableEntries.forEach((table, index) => {
    const entryOffset = 12 + index * 16;
    buffer.write(table.tag, entryOffset, 4, "ascii");
    buffer.writeUInt32BE(0, entryOffset + 4);
    buffer.writeUInt32BE(table.offset, entryOffset + 8);
    buffer.writeUInt32BE(table.data.length, entryOffset + 12);
    table.data.copy(buffer, table.offset);
  });

  return buffer;
}

function buildNameTable(records: NameRecordFixture[]): TableFixture {
  const recordBytes = Buffer.alloc(records.length * 12);
  const stringBuffers = records.map((record) => Buffer.from(record.value, "utf16le").swap16());
  const stringStorageOffset = 6 + recordBytes.length;
  let nextStringOffset = 0;

  records.forEach((record, index) => {
    const entryOffset = index * 12;
    const stringBuffer = stringBuffers[index]!;
    recordBytes.writeUInt16BE(3, entryOffset);
    recordBytes.writeUInt16BE(1, entryOffset + 2);
    recordBytes.writeUInt16BE(0x0409, entryOffset + 4);
    recordBytes.writeUInt16BE(record.nameId, entryOffset + 6);
    recordBytes.writeUInt16BE(stringBuffer.length, entryOffset + 8);
    recordBytes.writeUInt16BE(nextStringOffset, entryOffset + 10);
    nextStringOffset += stringBuffer.length;
  });

  const data = Buffer.concat([
    uint16(0),
    uint16(records.length),
    uint16(stringStorageOffset),
    recordBytes,
    ...stringBuffers
  ]);
  return { tag: "name", data };
}

function buildOs2Table(weight: number): TableFixture {
  const data = Buffer.alloc(6);
  data.writeUInt16BE(weight, 4);
  return { tag: "OS/2", data };
}

function buildFvarTable(options: { minWeight: number; maxWeight: number; instanceWeights: number[] }): TableFixture {
  const axisSize = 20;
  const instanceSize = 8;
  const axisCount = 1;
  const axesArrayOffset = 16;
  const instanceCount = options.instanceWeights.length;
  const data = Buffer.alloc(axesArrayOffset + axisSize * axisCount + instanceSize * instanceCount);

  data.writeUInt16BE(1, 0);
  data.writeUInt16BE(0, 2);
  data.writeUInt16BE(axesArrayOffset, 4);
  data.writeUInt16BE(2, 6);
  data.writeUInt16BE(axisCount, 8);
  data.writeUInt16BE(axisSize, 10);
  data.writeUInt16BE(instanceCount, 12);
  data.writeUInt16BE(instanceSize, 14);

  data.write("wght", axesArrayOffset, 4, "ascii");
  writeFixed1616(data, axesArrayOffset + 4, options.minWeight);
  writeFixed1616(data, axesArrayOffset + 8, 400);
  writeFixed1616(data, axesArrayOffset + 12, options.maxWeight);

  const instancesOffset = axesArrayOffset + axisSize;
  options.instanceWeights.forEach((weight, index) => {
    const instanceOffset = instancesOffset + index * instanceSize;
    data.writeUInt16BE(256 + index, instanceOffset);
    data.writeUInt16BE(0, instanceOffset + 2);
    writeFixed1616(data, instanceOffset + 4, weight);
  });

  return { tag: "fvar", data };
}

function uint16(value: number): Buffer {
  const buffer = Buffer.alloc(2);
  buffer.writeUInt16BE(value);
  return buffer;
}

function writeFixed1616(buffer: Buffer, offset: number, value: number): void {
  buffer.writeInt32BE(value * 65536, offset);
}
