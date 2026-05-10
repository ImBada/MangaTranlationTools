import type { RenderPageOptions } from "./pageRender";

export const OUTPUT_RENDER_OPTIONS: RenderPageOptions = {
  layerVisibility: {
    image: true,
    inpaint: true,
    inpaintResult: true,
    inpaintMask: false,
    overlay: true
  },
  layerOpacity: {
    image: 1,
    inpaint: 1,
    inpaintResult: 1,
    inpaintMask: 1,
    overlay: 1
  },
  activeLayer: "output"
};
