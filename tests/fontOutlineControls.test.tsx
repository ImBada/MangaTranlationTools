import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { FontOutlineControls } from "../src/client/src/components/font/FontOutlineControls";
import type { LinkableFontPresetKey } from "../src/client/src/lib/fontPresets";

describe("FontOutlineControls", () => {
  it("keeps the secondary outline controls visible when the enabled mode has zero width", () => {
    const markup = renderFontOutlineControls({
      secondaryOutlineEnabled: true,
      secondaryOutlineWidthPx: 0
    });

    expect(markup).toContain("2차 외곽선 색");
    expect(markup).toContain("2차 외곽선 두께");
    expect(markup).toContain("aria-pressed=\"true\">2개");
  });

  it("hides the secondary outline controls when the outline count is explicitly set to one", () => {
    const markup = renderFontOutlineControls({
      secondaryOutlineEnabled: false,
      secondaryOutlineWidthPx: 4
    });

    expect(markup).not.toContain("2차 외곽선 색");
    expect(markup).not.toContain("2차 외곽선 두께");
    expect(markup).toContain("aria-pressed=\"true\">1개");
  });

  it("renders a preset link control for outline count", () => {
    const renderLinkButton = vi.fn((key: LinkableFontPresetKey, label: string) => (
      <span data-link-key={key}>{label}</span>
    ));

    const markup = renderFontOutlineControls({}, renderLinkButton);

    expect(renderLinkButton).toHaveBeenCalledWith("secondaryOutlineEnabled", "외곽선 개수");
    expect(markup).toContain("data-link-key=\"secondaryOutlineEnabled\"");
  });
});

function renderFontOutlineControls(
  values: React.ComponentProps<typeof FontOutlineControls>["values"],
  renderLinkButton: React.ComponentProps<typeof FontOutlineControls>["renderLinkButton"] = () => null
): string {
  return renderToStaticMarkup(
    <FontOutlineControls
      disabled={false}
      onChange={() => undefined}
      renderLinkButton={renderLinkButton}
      values={{
        outlineColor: "#000000",
        outlineWidthPx: 1,
        secondaryOutlineColor: "#ffffff",
        secondaryOutlineWidthPx: 2,
        ...values
      }}
    />
  );
}
