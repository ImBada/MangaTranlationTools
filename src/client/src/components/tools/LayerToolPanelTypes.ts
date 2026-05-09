import type { FontCharacterOverride, TextDecoration, TextFontStyle } from "../../../../shared/types";

export type LayerToolFontControlValues = {
  autoFitText?: boolean;
  fontFamily?: string;
  characterFontOverrides?: FontCharacterOverride[];
  fontStyle?: TextFontStyle;
  fontWeight?: number;
  fontSizePx: number;
  lineHeight: number;
  letterSpacingPx?: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  screentoneFillAntialias?: boolean;
  screentoneFillDensity?: number;
  screentoneFillEnabled?: boolean;
  screentoneFillIntensity?: number;
  secondaryOutlineColor?: string;
  secondaryOutlineWidthPx?: number;
  shadowAngleDeg?: number;
  shadowColor?: string;
  shadowDistancePx?: number;
  shadowEnabled?: boolean;
  textDecoration?: TextDecoration;
  textColor?: string;
};
