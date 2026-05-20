import {
  DEFAULT_FILTER_BLUR_INTENSITY,
  DEFAULT_FILTER_GLITCH_INTENSITY,
  DEFAULT_FILTER_NOISE_INTENSITY,
  DEFAULT_FILTER_PIXELATE_INTENSITY,
  DEFAULT_FILTER_WAVE_INTENSITY,
  DEFAULT_FILTER_ZIGZAG_INTENSITY
} from "../../../shared/fontPresets";

export type FontFilterEffectValues = {
  filterEffectEnabled?: boolean;
  filterZigzagEnabled?: boolean;
  filterZigzagIntensity?: number;
  filterWaveEnabled?: boolean;
  filterWaveIntensity?: number;
  filterGlitchEnabled?: boolean;
  filterGlitchIntensity?: number;
  filterPixelateEnabled?: boolean;
  filterPixelateIntensity?: number;
  filterBlurEnabled?: boolean;
  filterBlurIntensity?: number;
  filterNoiseEnabled?: boolean;
  filterNoiseIntensity?: number;
};

export type FontFilterEffectControlEnabledKey =
  | "zigzagEnabled"
  | "waveEnabled"
  | "glitchEnabled"
  | "pixelateEnabled"
  | "blurEnabled"
  | "noiseEnabled";

export type FontFilterEffectControlIntensityKey =
  | "zigzagIntensity"
  | "waveIntensity"
  | "glitchIntensity"
  | "pixelateIntensity"
  | "blurIntensity"
  | "noiseIntensity";

export type FontFilterEffectControlPatch = Partial<
  Record<FontFilterEffectControlEnabledKey, boolean> & Record<FontFilterEffectControlIntensityKey, number>
>;

export const FILTER_EFFECT_INTENSITY_MIN = 0;
export const FILTER_EFFECT_INTENSITY_MAX = 1;
export const FILTER_EFFECT_INTENSITY_STEP = 0.01;

const ZIGZAG_DISPLACEMENT_FONT_SIZE_RATIO = 0.18;
const WAVE_DISPLACEMENT_FONT_SIZE_RATIO = 0.16;
const GLITCH_DISPLACEMENT_FONT_SIZE_RATIO = 0.14;
const GLITCH_CHANNEL_SPLIT_FONT_SIZE_RATIO = 0.055;
const PIXELATE_MIN_BLOCK_SIZE_PX = 2;
const PIXELATE_BLOCK_FONT_SIZE_RATIO = 0.26;
const BLUR_RADIUS_FONT_SIZE_RATIO = 0.08;

export type FontFilterEffectEnabledKey =
  | "filterZigzagEnabled"
  | "filterWaveEnabled"
  | "filterGlitchEnabled"
  | "filterPixelateEnabled"
  | "filterBlurEnabled"
  | "filterNoiseEnabled";

export type FontFilterEffectIntensityKey =
  | "filterZigzagIntensity"
  | "filterWaveIntensity"
  | "filterGlitchIntensity"
  | "filterPixelateIntensity"
  | "filterBlurIntensity"
  | "filterNoiseIntensity";

export type FontFilterEffectDefinition = {
  enabledControlKey: FontFilterEffectControlEnabledKey;
  enabledKey: FontFilterEffectEnabledKey;
  intensityKey: FontFilterEffectIntensityKey;
  intensityControlKey: FontFilterEffectControlIntensityKey;
  label: string;
  hasActive: (values: FontFilterEffectValues) => boolean;
  resolveIntensity: (intensity: number | undefined) => number;
  resolveEnabledIntensity: (intensity: number | undefined) => number;
  resolveMarginPx: (fontSizePx: number, intensity: number | undefined) => number;
};

export const FONT_FILTER_EFFECT_DEFINITIONS: readonly FontFilterEffectDefinition[] = [
  {
    enabledControlKey: "zigzagEnabled",
    enabledKey: "filterZigzagEnabled",
    intensityKey: "filterZigzagIntensity",
    intensityControlKey: "zigzagIntensity",
    label: "지그재그",
    hasActive: hasActiveFontZigzagFilter,
    resolveIntensity: resolveFilterZigzagIntensity,
    resolveEnabledIntensity: resolveEnabledFontZigzagIntensity,
    resolveMarginPx: (fontSizePx, intensity) => Math.ceil(resolveZigzagDisplacementPx(fontSizePx, intensity) * 2 + 2)
  },
  {
    enabledControlKey: "waveEnabled",
    enabledKey: "filterWaveEnabled",
    intensityKey: "filterWaveIntensity",
    intensityControlKey: "waveIntensity",
    label: "물결",
    hasActive: hasActiveFontWaveFilter,
    resolveIntensity: resolveFilterWaveIntensity,
    resolveEnabledIntensity: resolveEnabledFontWaveIntensity,
    resolveMarginPx: (fontSizePx, intensity) => Math.ceil(resolveWaveDisplacementPx(fontSizePx, intensity) * 2 + 2)
  },
  {
    enabledControlKey: "glitchEnabled",
    enabledKey: "filterGlitchEnabled",
    intensityKey: "filterGlitchIntensity",
    intensityControlKey: "glitchIntensity",
    label: "글리치",
    hasActive: hasActiveFontGlitchFilter,
    resolveIntensity: resolveFilterGlitchIntensity,
    resolveEnabledIntensity: resolveEnabledFontGlitchIntensity,
    resolveMarginPx: resolveGlitchMarginPx
  },
  {
    enabledControlKey: "pixelateEnabled",
    enabledKey: "filterPixelateEnabled",
    intensityKey: "filterPixelateIntensity",
    intensityControlKey: "pixelateIntensity",
    label: "픽셀화",
    hasActive: hasActiveFontPixelateFilter,
    resolveIntensity: resolveFilterPixelateIntensity,
    resolveEnabledIntensity: resolveEnabledFontPixelateIntensity,
    resolveMarginPx: () => 0
  },
  {
    enabledControlKey: "blurEnabled",
    enabledKey: "filterBlurEnabled",
    intensityKey: "filterBlurIntensity",
    intensityControlKey: "blurIntensity",
    label: "번짐",
    hasActive: hasActiveFontBlurFilter,
    resolveIntensity: resolveFilterBlurIntensity,
    resolveEnabledIntensity: resolveEnabledFontBlurIntensity,
    resolveMarginPx: (fontSizePx, intensity) => resolveBlurRadiusPx(fontSizePx, intensity) * 3 + 2
  },
  {
    enabledControlKey: "noiseEnabled",
    enabledKey: "filterNoiseEnabled",
    intensityKey: "filterNoiseIntensity",
    intensityControlKey: "noiseIntensity",
    label: "노이즈",
    hasActive: hasActiveFontNoiseFilter,
    resolveIntensity: resolveFilterNoiseIntensity,
    resolveEnabledIntensity: resolveEnabledFontNoiseIntensity,
    resolveMarginPx: () => 0
  }
];

export function resolveFilterZigzagIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_ZIGZAG_INTENSITY);
}

export function resolveFilterWaveIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_WAVE_INTENSITY);
}

export function resolveFilterGlitchIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_GLITCH_INTENSITY);
}

export function resolveFilterPixelateIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_PIXELATE_INTENSITY);
}

export function resolveFilterBlurIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_BLUR_INTENSITY);
}

export function resolveFilterNoiseIntensity(intensity: number | undefined): number {
  return resolveFilterIntensity(intensity, DEFAULT_FILTER_NOISE_INTENSITY);
}

export function resolveEnabledFontZigzagIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_ZIGZAG_INTENSITY);
}

export function resolveEnabledFontWaveIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_WAVE_INTENSITY);
}

export function resolveEnabledFontGlitchIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_GLITCH_INTENSITY);
}

export function resolveEnabledFontPixelateIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_PIXELATE_INTENSITY);
}

export function resolveEnabledFontBlurIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_BLUR_INTENSITY);
}

export function resolveEnabledFontNoiseIntensity(intensity: number | undefined): number {
  return resolveEnabledFontFilterIntensity(intensity, DEFAULT_FILTER_NOISE_INTENSITY);
}

export function hasActiveFontZigzagFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterZigzagEnabled", "filterZigzagIntensity", resolveFilterZigzagIntensity);
}

export function hasActiveFontWaveFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterWaveEnabled", "filterWaveIntensity", resolveFilterWaveIntensity);
}

export function hasActiveFontGlitchFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterGlitchEnabled", "filterGlitchIntensity", resolveFilterGlitchIntensity);
}

export function hasActiveFontPixelateFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterPixelateEnabled", "filterPixelateIntensity", resolveFilterPixelateIntensity);
}

export function hasActiveFontBlurFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterBlurEnabled", "filterBlurIntensity", resolveFilterBlurIntensity);
}

export function hasActiveFontNoiseFilter(values: FontFilterEffectValues): boolean {
  return hasActiveFilter(values, "filterNoiseEnabled", "filterNoiseIntensity", resolveFilterNoiseIntensity);
}

export function hasActiveFontFilterEffects(values: FontFilterEffectValues): boolean {
  return FONT_FILTER_EFFECT_DEFINITIONS.some((definition) => definition.hasActive(values));
}

export function buildEnabledFontFilterEffectSettingPatch(values: FontFilterEffectValues): Partial<FontFilterEffectValues> {
  if (hasActiveFontFilterChild(values)) {
    return { filterEffectEnabled: true };
  }
  return {
    filterEffectEnabled: true,
    filterZigzagEnabled: true,
    filterZigzagIntensity: resolveEnabledFontZigzagIntensity(values.filterZigzagIntensity)
  };
}

export function buildFontFilterEffectSettingPatch(patch: FontFilterEffectControlPatch): Partial<FontFilterEffectValues> {
  const values: Partial<FontFilterEffectValues> = {};
  for (const definition of FONT_FILTER_EFFECT_DEFINITIONS) {
    const enabled = patch[definition.enabledControlKey];
    if (enabled !== undefined) {
      Object.assign(values, { [definition.enabledKey]: enabled });
    }
    const intensity = patch[definition.intensityControlKey];
    if (intensity !== undefined) {
      Object.assign(values, { [definition.intensityKey]: intensity });
    }
  }
  return values;
}

export function resolveFontFilterFontSizePx(fontSizePx: number): number {
  return Number.isFinite(fontSizePx) ? Math.max(1, fontSizePx) : 1;
}

export function resolveZigzagDisplacementPx(fontSizePx: number, intensity: number | undefined): number {
  return resolveFontFilterFontSizePx(fontSizePx) * ZIGZAG_DISPLACEMENT_FONT_SIZE_RATIO * resolveFilterZigzagIntensity(intensity);
}

export function resolveWaveDisplacementPx(fontSizePx: number, intensity: number | undefined): number {
  return resolveFontFilterFontSizePx(fontSizePx) * WAVE_DISPLACEMENT_FONT_SIZE_RATIO * resolveFilterWaveIntensity(intensity);
}

export function resolveGlitchDisplacementPx(fontSizePx: number, intensity: number | undefined): number {
  return resolveFontFilterFontSizePx(fontSizePx) * GLITCH_DISPLACEMENT_FONT_SIZE_RATIO * resolveFilterGlitchIntensity(intensity);
}

export function resolveGlitchChannelSplitPx(fontSizePx: number, intensity: number | undefined): number {
  return resolveFontFilterFontSizePx(fontSizePx) * GLITCH_CHANNEL_SPLIT_FONT_SIZE_RATIO * resolveFilterGlitchIntensity(intensity);
}

export function resolvePixelateBlockSizePx(fontSizePx: number, intensity: number | undefined): number {
  const resolvedIntensity = resolveFilterPixelateIntensity(intensity);
  if (resolvedIntensity <= 0) {
    return 1;
  }
  return Math.max(
    PIXELATE_MIN_BLOCK_SIZE_PX,
    Math.round(resolveFontFilterFontSizePx(fontSizePx) * PIXELATE_BLOCK_FONT_SIZE_RATIO * resolvedIntensity)
  );
}

export function resolveBlurRadiusPx(fontSizePx: number, intensity: number | undefined): number {
  const resolvedIntensity = resolveFilterBlurIntensity(intensity);
  return resolvedIntensity > 0
    ? Math.max(1, Math.round(resolveFontFilterFontSizePx(fontSizePx) * BLUR_RADIUS_FONT_SIZE_RATIO * resolvedIntensity))
    : 0;
}

export function resolveFontFilterEffectMarginPx(values: FontFilterEffectValues, fontSizePx: number): number {
  if (!values.filterEffectEnabled) {
    return 0;
  }
  const marginPx = FONT_FILTER_EFFECT_DEFINITIONS.reduce((total, definition) => {
    if (!definition.hasActive(values)) {
      return total;
    }
    return total + definition.resolveMarginPx(fontSizePx, values[definition.intensityKey]);
  }, 0);
  return Math.ceil(marginPx);
}

function resolveGlitchMarginPx(fontSizePx: number, intensity: number | undefined): number {
  return Math.ceil((resolveGlitchDisplacementPx(fontSizePx, intensity) + resolveGlitchChannelSplitPx(fontSizePx, intensity)) * 2 + 2);
}

function resolveFilterIntensity(intensity: number | undefined, defaultIntensity: number): number {
  const value = intensity ?? defaultIntensity;
  return Number.isFinite(value)
    ? Math.max(FILTER_EFFECT_INTENSITY_MIN, Math.min(FILTER_EFFECT_INTENSITY_MAX, value))
    : defaultIntensity;
}

function resolveEnabledFontFilterIntensity(intensity: number | undefined, defaultIntensity: number): number {
  const resolvedIntensity = resolveFilterIntensity(intensity, defaultIntensity);
  return resolvedIntensity > 0 ? resolvedIntensity : defaultIntensity;
}

function hasActiveFilter(
  values: FontFilterEffectValues,
  enabledKey: FontFilterEffectEnabledKey,
  intensityKey: FontFilterEffectIntensityKey,
  resolveIntensity: (intensity: number | undefined) => number
): boolean {
  return Boolean(values.filterEffectEnabled && values[enabledKey] && resolveIntensity(values[intensityKey]) > 0);
}

function hasActiveFontFilterChild(values: FontFilterEffectValues): boolean {
  return FONT_FILTER_EFFECT_DEFINITIONS.some((definition) =>
    Boolean(values[definition.enabledKey] && definition.resolveIntensity(values[definition.intensityKey]) > 0)
  );
}
