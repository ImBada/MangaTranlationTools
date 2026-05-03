import type { TextDecoration, TextFontStyle } from "../../../../shared/types";

export type LayerToolFontControlValues = {
  autoFitText?: boolean;
  fontFamily?: string;
  fontStyle?: TextFontStyle;
  fontWeight?: number;
  fontSizePx: number;
  lineHeight: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  screentoneFillAntialias?: boolean;
  screentoneFillDensity?: number;
  screentoneFillEnabled?: boolean;
  screentoneFillIntensity?: number;
  secondaryOutlineColor?: string;
  secondaryOutlineWidthPx?: number;
  textDecoration?: TextDecoration;
  textColor?: string;
};
