import { describe, expect, it } from "vitest";
import type { MangaPage } from "../../../shared/types";
import {
  isExistingTranslationBlockGroupSelection,
  resolveExpandedTranslationBlockSelection,
  resolveTranslationBlockGroupBlockIds,
  resolveTranslationBlockListItems,
  resolveTranslationBlockGroupsAfterBlockRemoval,
  resolveTranslationBlockGroupsAfterGrouping,
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

  it("expands selection and list items by valid groups", () => {
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

    expect(resolveTranslationBlockGroupBlockIds(page, "b3")).toEqual(["b1", "b3"]);
    expect(resolveExpandedTranslationBlockSelection(page, ["b3", "b4"])).toEqual(["b1", "b3", "b4"]);
    expect(resolveTranslationBlockListItems(page)).toEqual([
      {
        kind: "group",
        group: {
          id: "group-1",
          blockIds: ["b1", "b3"],
          effects: [],
          createdAt: "2026-05-10T10:00:00.000Z",
          updatedAt: "2026-05-10T10:00:00.000Z"
        },
        blocks: [
          { block: block("b1"), blockIndex: 0 },
          { block: block("b3"), blockIndex: 2 }
        ]
      },
      { kind: "block", block: block("b2"), blockIndex: 1 },
      { kind: "block", block: block("b4"), blockIndex: 3 }
    ]);
    expect(isExistingTranslationBlockGroupSelection(page, ["b3", "b1"])).toBe(true);
    expect(isExistingTranslationBlockGroupSelection(page, ["b1", "b2", "b3"])).toBe(false);
  });
});
