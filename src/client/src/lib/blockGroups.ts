import type { MangaPage, TranslationBlock, TranslationBlockGroup, TranslationBlockGroupEffect } from "../../../shared/types";
import { cloneTranslationBlockGroupEffect } from "./blockGroupEffects";

export type TranslationBlockListItem =
  | {
      kind: "block";
      block: TranslationBlock;
      blockIndex: number;
    }
  | {
      kind: "group";
      group: TranslationBlockGroup;
      blocks: {
        block: TranslationBlock;
        blockIndex: number;
      }[];
    };

export function createTranslationBlockGroupId(): string {
  const randomUUID = globalThis.crypto?.randomUUID?.bind(globalThis.crypto);
  return `text-block-group-${randomUUID ? randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`}`;
}

export function cloneTranslationBlockGroup(group: TranslationBlockGroup): TranslationBlockGroup {
  return {
    ...group,
    blockIds: [...group.blockIds],
    effects: (group.effects ?? []).map(cloneTranslationBlockGroupEffect)
  };
}

export function createTranslationBlockGroup(
  id: string,
  blockIds: readonly string[],
  now: string
): TranslationBlockGroup {
  return {
    id,
    blockIds: Array.from(new Set(blockIds)),
    effects: [],
    createdAt: now,
    updatedAt: now
  };
}

export function resolveTranslationBlockGroupsAfterGrouping(
  page: Pick<MangaPage, "blocks" | "blockGroups">,
  selectedBlockIds: readonly string[],
  groupId: string,
  now: string
): TranslationBlockGroup[] | null {
  const selectedBlockIdSet = new Set(selectedBlockIds);
  const groupedBlockIds = page.blocks
    .filter((block) => selectedBlockIdSet.has(block.id))
    .map((block) => block.id);

  if (groupedBlockIds.length < 2) {
    return null;
  }

  const normalizedGroups = resolveValidTranslationBlockGroups(page);
  const matchingGroupIndex = normalizedGroups.findIndex((group) => blockIdSetsMatch(group.blockIds, groupedBlockIds));
  if (matchingGroupIndex >= 0) {
    return normalizedGroups.map((group, index) =>
      index === matchingGroupIndex && !blockIdArraysMatch(group.blockIds, groupedBlockIds)
        ? { ...group, blockIds: groupedBlockIds, updatedAt: now }
        : group
    );
  }

  const groupedBlockIdSet = new Set(groupedBlockIds);
  const retainedGroups = normalizedGroups.flatMap((group) => {
    const nextBlockIds = group.blockIds.filter((blockId) => !groupedBlockIdSet.has(blockId));
    if (nextBlockIds.length < 2) {
      return [];
    }

    return [{
      ...group,
      blockIds: nextBlockIds,
      updatedAt: nextBlockIds.length === group.blockIds.length ? group.updatedAt : now
    }];
  });

  return [
    ...retainedGroups,
    createTranslationBlockGroup(groupId, groupedBlockIds, now)
  ];
}

export function resolveTranslationBlockGroupsAfterBlockRemoval(
  groups: readonly TranslationBlockGroup[] | undefined,
  removedBlockIds: readonly string[],
  now: string
): TranslationBlockGroup[] | undefined {
  if (!groups?.length) {
    return undefined;
  }

  const removedBlockIdSet = new Set(removedBlockIds);
  const nextGroups = groups.flatMap((group) => {
    const nextBlockIds = group.blockIds.filter((blockId) => !removedBlockIdSet.has(blockId));
    if (nextBlockIds.length < 2) {
      return [];
    }

    const nextGroup = cloneTranslationBlockGroup(group);
    return [{
      ...nextGroup,
      blockIds: nextBlockIds,
      updatedAt: nextBlockIds.length === group.blockIds.length ? nextGroup.updatedAt : now
    }];
  });

  return nextGroups.length > 0 ? nextGroups : undefined;
}

export function resolveTranslationBlockGroupsAfterUngrouping(
  page: Pick<MangaPage, "blocks" | "blockGroups">,
  selectedBlockIds: readonly string[]
): TranslationBlockGroup[] | undefined | null {
  if (selectedBlockIds.length < 2) {
    return null;
  }

  const groups = resolveValidTranslationBlockGroups(page);
  const selectedGroup = resolveTranslationBlockGroupForSelection(groups, selectedBlockIds);
  if (!selectedGroup) {
    return null;
  }

  const nextGroups = groups.filter((group) => group.id !== selectedGroup.id);
  return nextGroups.length > 0 ? nextGroups : undefined;
}

export function resolveTranslationBlockGroupsAfterReordering(
  page: Pick<MangaPage, "blocks" | "blockGroups">,
  groupId: string,
  blockIds: readonly string[],
  now: string
): TranslationBlockGroup[] | null {
  const groups = resolveValidTranslationBlockGroups(page);
  const targetGroup = groups.find((group) => group.id === groupId);
  if (!targetGroup) {
    return null;
  }

  const validBlockIdSet = new Set(targetGroup.blockIds);
  const seenBlockIds = new Set<string>();
  const nextBlockIds = blockIds.filter((blockId) => {
    if (!validBlockIdSet.has(blockId) || seenBlockIds.has(blockId)) {
      return false;
    }
    seenBlockIds.add(blockId);
    return true;
  });

  if (!blockIdSetsMatch(targetGroup.blockIds, nextBlockIds) || blockIdArraysMatch(targetGroup.blockIds, nextBlockIds)) {
    return null;
  }

  return groups.map((group) =>
    group.id === groupId
      ? {
          ...group,
          blockIds: nextBlockIds,
          updatedAt: now
        }
      : group
  );
}

export function resolveTranslationBlocksAfterGroupReordering(
  blocks: readonly TranslationBlock[],
  blockIds: readonly string[]
): TranslationBlock[] | null {
  const blockById = new Map(blocks.map((block) => [block.id, block]));
  const seenBlockIds = new Set<string>();
  const reorderedBlocks = blockIds.flatMap((blockId) => {
    if (seenBlockIds.has(blockId)) {
      return [];
    }
    seenBlockIds.add(blockId);
    const block = blockById.get(blockId);
    return block ? [block] : [];
  });

  if (reorderedBlocks.length !== seenBlockIds.size || reorderedBlocks.length < 2) {
    return null;
  }

  const reorderedBlockIdSet = new Set(reorderedBlocks.map((block) => block.id));
  let reorderedBlockIndex = 0;
  const nextBlocks = blocks.map((block) =>
    reorderedBlockIdSet.has(block.id)
      ? reorderedBlocks[reorderedBlockIndex++] ?? block
      : block
  );

  return nextBlocks.every((block, index) => block === blocks[index]) ? null : nextBlocks;
}

export function resolveTranslationBlocksAfterReordering(
  blocks: readonly TranslationBlock[],
  blockIds: readonly string[]
): TranslationBlock[] | null {
  const blockById = new Map(blocks.map((block) => [block.id, block]));
  const seenBlockIds = new Set<string>();
  const nextBlocks = blockIds.flatMap((blockId) => {
    if (seenBlockIds.has(blockId)) {
      return [];
    }
    seenBlockIds.add(blockId);
    const block = blockById.get(blockId);
    return block ? [block] : [];
  });

  if (nextBlocks.length !== blocks.length || nextBlocks.length !== seenBlockIds.size) {
    return null;
  }

  const currentBlockIds = blocks.map((block) => block.id);
  if (!blockIdSetsMatch(currentBlockIds, blockIds) || blockIdArraysMatch(currentBlockIds, blockIds)) {
    return null;
  }

  return nextBlocks;
}

export function resolveTranslationBlockGroupBlockIds(
  page: Pick<MangaPage, "blocks" | "blockGroups"> | null,
  blockId: string
): string[] | null {
  if (!page?.blockGroups?.length) {
    return null;
  }

  return resolveValidTranslationBlockGroups(page).find((group) => group.blockIds.includes(blockId))?.blockIds ?? null;
}

export function resolveExpandedTranslationBlockSelection(
  page: Pick<MangaPage, "blocks" | "blockGroups"> | null,
  blockIds: readonly string[]
): string[] {
  if (!page || blockIds.length === 0) {
    return [];
  }

  const selectedBlockIdSet = new Set(blockIds);
  for (const blockId of blockIds) {
    const groupBlockIds = resolveTranslationBlockGroupBlockIds(page, blockId);
    groupBlockIds?.forEach((groupBlockId) => selectedBlockIdSet.add(groupBlockId));
  }

  return page.blocks
    .filter((block) => selectedBlockIdSet.has(block.id))
    .map((block) => block.id);
}

export function resolveTranslationBlockListItems(
  page: Pick<MangaPage, "blocks" | "blockGroups">
): TranslationBlockListItem[] {
  const blocksById = new Map(page.blocks.map((block) => [block.id, block]));
  const blockIndexById = new Map(page.blocks.map((block, index) => [block.id, index]));
  const groups = resolveValidTranslationBlockGroups(page);
  const groupByBlockId = new Map<string, TranslationBlockGroup>();
  const renderedGroupIds = new Set<string>();
  const items: TranslationBlockListItem[] = [];

  groups.forEach((group) => {
    group.blockIds.forEach((blockId) => {
      if (!groupByBlockId.has(blockId)) {
        groupByBlockId.set(blockId, group);
      }
    });
  });

  page.blocks.forEach((block, blockIndex) => {
    const group = groupByBlockId.get(block.id);
    if (!group) {
      items.push({ kind: "block", block, blockIndex });
      return;
    }
    if (renderedGroupIds.has(group.id)) {
      return;
    }

    renderedGroupIds.add(group.id);
    items.push({
      kind: "group",
      group,
      blocks: group.blockIds.flatMap((groupBlockId) => {
        const groupBlock = blocksById.get(groupBlockId);
        const groupBlockIndex = blockIndexById.get(groupBlockId) ?? -1;
        return groupBlock && groupBlockIndex >= 0
          ? [{ block: groupBlock, blockIndex: groupBlockIndex }]
          : [];
      })
    });
  });

  return items;
}

export function isExistingTranslationBlockGroupSelection(
  page: Pick<MangaPage, "blocks" | "blockGroups">,
  selectedBlockIds: readonly string[]
): boolean {
  return resolveSelectedTranslationBlockGroup(page, selectedBlockIds) !== null;
}

export function resolveSelectedTranslationBlockGroup(
  page: Pick<MangaPage, "blocks" | "blockGroups">,
  selectedBlockIds: readonly string[]
): TranslationBlockGroup | null {
  return resolveTranslationBlockGroupForSelection(resolveValidTranslationBlockGroups(page), selectedBlockIds);
}

function resolveTranslationBlockGroupForSelection(
  groups: readonly TranslationBlockGroup[],
  selectedBlockIds: readonly string[]
): TranslationBlockGroup | null {
  if (selectedBlockIds.length < 2) {
    return null;
  }

  return groups.find((group) => blockIdSetsMatch(group.blockIds, selectedBlockIds)) ?? null;
}

export function translationBlockGroupsEqual(
  left: readonly TranslationBlockGroup[] | undefined,
  right: readonly TranslationBlockGroup[] | undefined
): boolean {
  const leftGroups = left ?? [];
  const rightGroups = right ?? [];
  if (leftGroups.length !== rightGroups.length) {
    return false;
  }

  return leftGroups.every((leftGroup, index) => translationBlockGroupEqual(leftGroup, rightGroups[index]));
}

function resolveValidTranslationBlockGroups(
  page: Pick<MangaPage, "blocks" | "blockGroups">
): TranslationBlockGroup[] {
  const blockIdSet = new Set(page.blocks.map((block) => block.id));
  return (page.blockGroups ?? []).flatMap((group) => {
    const seenBlockIds = new Set<string>();
    const blockIds = group.blockIds.filter((blockId) => {
      if (!blockIdSet.has(blockId) || seenBlockIds.has(blockId)) {
        return false;
      }
      seenBlockIds.add(blockId);
      return true;
    });

    return blockIds.length >= 2
      ? [{ ...cloneTranslationBlockGroup(group), blockIds }]
      : [];
  });
}

function blockIdSetsMatch(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  const rightSet = new Set(right);
  return left.every((blockId) => rightSet.has(blockId));
}

function blockIdArraysMatch(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((blockId, index) => blockId === right[index]);
}

function translationBlockGroupEqual(
  left: TranslationBlockGroup,
  right: TranslationBlockGroup | undefined
): boolean {
  if (!right) {
    return false;
  }

  return (
    left.id === right.id &&
    left.createdAt === right.createdAt &&
    left.updatedAt === right.updatedAt &&
    blockIdArraysMatch(left.blockIds, right.blockIds) &&
    translationBlockGroupEffectsEqual(left.effects, right.effects)
  );
}

function translationBlockGroupEffectsEqual(
  left: readonly TranslationBlockGroupEffect[],
  right: readonly TranslationBlockGroupEffect[]
): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((effect, index) => {
    const rightEffect = right[index];
    return Boolean(rightEffect) &&
      effect.id === rightEffect.id &&
      effect.type === rightEffect.type &&
      effect.enabled === rightEffect.enabled &&
      JSON.stringify(effect.settings ?? {}) === JSON.stringify(rightEffect.settings ?? {});
  });
}
