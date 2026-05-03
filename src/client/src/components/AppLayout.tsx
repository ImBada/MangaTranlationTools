import React from "react";

type AppLayoutProps = {
  currentChapterPresent: boolean;
  fileInputs: React.ReactNode;
  contextBar: React.ReactNode;
  pageList: React.ReactNode;
  workspace: React.ReactNode;
  layerTools: React.ReactNode;
  rightRail: React.ReactNode;
  modals: React.ReactNode;
};

export function AppLayout({
  contextBar,
  currentChapterPresent,
  fileInputs,
  layerTools,
  modals,
  pageList,
  rightRail,
  workspace
}: AppLayoutProps): React.JSX.Element {
  return (
    <main className={currentChapterPresent ? "app-shell grid h-screen bg-canvas" : "app-shell no-left-rail grid h-screen bg-canvas"}>
      {fileInputs}
      {contextBar}

      <aside className="sidebar flex min-h-0 flex-col gap-3 overflow-hidden">
        {pageList}
      </aside>

      {workspace}

      <aside className="layer-tools-rail flex min-h-0 flex-col gap-3 overflow-hidden">
        {layerTools}
      </aside>

      <aside className="right-rail flex min-h-0 flex-col gap-3 overflow-hidden">
        {rightRail}
      </aside>

      {modals}
    </main>
  );
}
