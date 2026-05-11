import { describe, expect, it } from "vitest";
import type { MangaPage } from "../../../shared/types";
import {
  isExistingTranslationBlockGroupSelection,
  resolveExpandedTranslationBlockSelection,
  resolveSelectedTranslationBlockGroup,
  resolveTranslationBlocksAfterGroupReordering,
  resolveTranslationBlocksAfterReordering,
  resolveTranslationBlockDragBlockIds,
  resolveTranslationBlockGroupBlockIds,
  resolveTranslationBlockListItems,
  resolveTranslationBlockGroupsAfterBlockRemoval,
  resolveTranslationBlockGroupsAfterGrouping,
  resolveTranslationBlockGroupsAfterReordering,
  resolveTranslationBlockGroupsAfterUngrouping
} from "./blockGroups";

function block(id: string): MangaPage["blocks"][number] {
  return { id } as MangaPage["blocks"][number];
}

describe("blockGroups", () => {
  it("groups selected blocks in page order and removes them from existing groups", () => {
    const now = "2026-05-11T10:00:00.000Z";
    const groups = resolveTranslationBlockGroupsAfterGrouping(
      {
        blocks: [block("b1"), block("b2"), block("b3"), block("b4"), block("b5")],
        blockGroups: [
          {
            id: "existing",
            blockIds: ["b2", "b4", "b5"],
            effects: [{ id: "effect-1", type: "sample", enabled: true, settings: { strength: 1 } }],
            createdAt: "2026-05-10T10:00:00.000Z",
            updatedAt: "2026-05-10T10:00:00.000Z"
          }
        ]
      },
      ["b3", "b1", "b2"],
      "group-new",
      now
    );

    expect(groups).toEqual([
      {
        id: "existing",
        blockIds: ["b4", "b5"],
        effects: [{ id: "effect-1", type: "sample", enabled: true, settings: { strength: 1 } }],
        createdAt: "2026-05-10T10:00:00.000Z",
        updatedAt: now
      },
      {
        id: "group-new",
        blockIds: ["b1", "b2", "b3"],
        effects: [],
        createdAt: now,
        updatedAt: now
      }
    ]);
  });

  it("does not create a group with fewer than two valid selected blocks", () => {
    const groups = resolveTranslationBlockGroupsAfterGrouping(
      { blocks: [block("b1"), block("b2")], blockGroups: undefined },
      ["b1", "missing"],
      "group-new",
      "2026-05-11T10:00:00.000Z"
    );

    expect(groups).toBeNull();
  });

  it("drops groups that no longer have enough blocks after deletion", () => {
    const now = "2026-05-11T10:00:00.000Z";
    const groups = resolveTranslationBlockGroupsAfterBlockRemoval(
      [
        {
          id: "keep",
          blockIds: ["b1", "b2", "b3"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        },
        {
          id: "drop",
          blockIds: ["b4", "b5"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        }
      ],
      ["b3", "b4"],
      now
    );

    expect(groups).toEqual([
      {
        id: "keep",
        blockIds: ["b1", "b2"],
        effects: [],
        createdAt: "2026-05-10T10:00:00.000Z",
        updatedAt: now
      }
    ]);
  });

  it("removes the selected existing group when ungrouping", () => {
    const page = {
      blocks: [block("b1"), block("b2"), block("b3"), block("b4")],
      blockGroups: [
        {
          id: "remove",
          blockIds: ["b3", "b1"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        },
        {
          id: "keep",
          blockIds: ["b2", "b4"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        }
      ]
    };

    expect(resolveTranslationBlockGroupsAfterUngrouping(page, ["b1", "b3"])).toEqual([
      {
        id: "keep",
        blockIds: ["b2", "b4"],
        effects: [],
        createdAt: "2026-05-10T10:00:00.000Z",
        updatedAt: "2026-05-10T10:00:00.000Z"
      }
    ]);
    expect(resolveTranslationBlockGroupsAfterUngrouping(page, ["b1", "b2"])).toBeNull();
  });

  it("reorders a group while preserving its block membership", () => {
    const now = "2026-05-11T10:00:00.000Z";
    const page = {
      blocks: [block("b1"), block("b2"), block("b3"), block("b4"), block("b5")],
      blockGroups: [
        {
          id: "group-1",
          blockIds: ["b1", "b2", "b3"],
          effects: [{ id: "effect-1", type: "sample", enabled: true, settings: { strength: 1 } }],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        },
        {
          id: "group-2",
          blockIds: ["b4", "b5"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        }
      ]
    };

    expect(resolveTranslationBlockGroupsAfterReordering(page, "group-1", ["b3", "b1", "b2"], now)).toEqual([
      {
        id: "group-1",
        blockIds: ["b3", "b1", "b2"],
        effects: [{ id: "effect-1", type: "sample", enabled: true, settings: { strength: 1 } }],
        createdAt: "2026-05-10T10:00:00.000Z",
        updatedAt: now
      },
      {
        id: "group-2",
        blockIds: ["b4", "b5"],
        effects: [],
        createdAt: "2026-05-10T10:00:00.000Z",
        updatedAt: "2026-05-10T10:00:00.000Z"
      }
    ]);
    expect(resolveTranslationBlockGroupsAfterReordering(page, "group-1", ["b3", "b1"], now)).toBeNull();
    expect(resolveTranslationBlockGroupsAfterReordering(page, "group-1", ["b1", "b2", "b3"], now)).toBeNull();
  });

  it("reorders page blocks in the same group slots for output stacking", () => {
    const blocks = [block("b1"), block("outside-1"), block("b2"), block("outside-2"), block("b3")];

    expect(resolveTranslationBlocksAfterGroupReordering(blocks, ["b3", "b1", "b2"])?.map((candidate) => candidate.id)).toEqual([
      "b3",
      "outside-1",
      "b1",
      "outside-2",
      "b2"
    ]);
    expect(resolveTranslationBlocksAfterGroupReordering(blocks, ["b1", "b2", "b3"])).toBeNull();
    expect(resolveTranslationBlocksAfterGroupReordering(blocks, ["b1", "missing"])).toBeNull();
  });

  it("reorders the full page block stack", () => {
    const blocks = [block("b1"), block("b2"), block("b3")];

    expect(resolveTranslationBlocksAfterReordering(blocks, ["b3", "b1", "b2"])?.map((candidate) => candidate.id)).toEqual([
      "b3",
      "b1",
      "b2"
    ]);
    expect(resolveTranslationBlocksAfterReordering(blocks, ["b1", "b2", "b3"])).toBeNull();
    expect(resolveTranslationBlocksAfterReordering(blocks, ["b1", "b2"])).toBeNull();
    expect(resolveTranslationBlocksAfterReordering(blocks, ["b1", "b2", "missing"])).toBeNull();
  });

  it("expands selection and list items by valid groups while preserving group order", () => {
    const page = {
      blocks: [block("b1"), block("b2"), block("b3"), block("b4")],
      blockGroups: [
        {
          id: "group-1",
          blockIds: ["b3", "b1", "missing"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        }
      ]
    };

    expect(resolveTranslationBlockGroupBlockIds(page, "b3")).toEqual(["b3", "b1"]);
    expect(resolveExpandedTranslationBlockSelection(page, ["b3", "b4"])).toEqual(["b1", "b3", "b4"]);
    expect(resolveTranslationBlockListItems(page)).toEqual([
      {
        kind: "group",
        group: {
          id: "group-1",
          blockIds: ["b3", "b1"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        },
        blocks: [
          { block: block("b3"), blockIndex: 2 },
          { block: block("b1"), blockIndex: 0 }
        ]
      },
      { kind: "block", block: block("b2"), blockIndex: 1 },
      { kind: "block", block: block("b4"), blockIndex: 3 }
    ]);
    expect(isExistingTranslationBlockGroupSelection(page, ["b3", "b1"])).toBe(true);
    expect(resolveSelectedTranslationBlockGroup(page, ["b3", "b1"])).toMatchObject({
      id: "group-1",
      blockIds: ["b3", "b1"]
    });
    expect(isExistingTranslationBlockGroupSelection(page, ["b1", "b2", "b3"])).toBe(false);
  });

  it("resolves drag targets for selected groups and unselected group members", () => {
    const page = {
      blocks: [block("b1"), block("b2"), block("b3")],
      blockGroups: [
        {
          id: "group-1",
          blockIds: ["b1", "b3"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        }
      ]
    };

    expect(resolveTranslationBlockDragBlockIds(page, "b1", null, ["b1", "b3"])).toEqual(["b1", "b3"]);
    expect(resolveTranslationBlockDragBlockIds(page, "b1", null, [])).toEqual(["b1", "b3"]);
    expect(resolveTranslationBlockDragBlockIds(page, "b1", "b1", [])).toEqual(["b1"]);
    expect(resolveTranslationBlockDragBlockIds(page, "b1", null, ["b1", "b2"])).toEqual(["b1", "b3"]);
  });
});
