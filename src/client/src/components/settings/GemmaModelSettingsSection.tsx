import React from "react";
import type { ModelSource } from "../../../../shared/types";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";
import {
  MAX_GPU_LAYERS,
  MODEL_PRESETS,
  MODEL_SOURCE_OPTIONS,
  type ModelPresetId
} from "./settingsModalConfig";
import { clampGpuLayers } from "./settingsModalUtils";

type GemmaModelSettingsSectionProps = {
  controlsBusy: boolean;
  customModelFile: string;
  customModelRepo: string;
  gpuLayers: string;
  gpuSliderRef: React.RefObject<HTMLInputElement | null>;
  localMmprojPath: string;
  localModelInputRef: React.RefObject<HTMLInputElement | null>;
  localModelPath: string;
  modelRepoInputRef: React.RefObject<HTMLInputElement | null>;
  modelSource: ModelSource;
  selectedPreset: ModelPresetId;
  sliderValue: number;
  onClearTestState: () => void;
  onCustomModelFileChange: (value: string) => void;
  onCustomModelRepoChange: (value: string) => void;
  onGpuLayersChange: (value: string) => void;
  onGpuLayersInputChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
  onLocalMmprojPathChange: (value: string) => void;
  onLocalModelPathChange: (value: string) => void;
  onModelSourceChange: (value: ModelSource) => void;
  onSelectedPresetChange: (value: ModelPresetId) => void;
  onSubmit: () => void;
};

export function GemmaModelSettingsSection({
  controlsBusy,
  customModelFile,
  customModelRepo,
  gpuLayers,
  gpuSliderRef,
  localMmprojPath,
  localModelInputRef,
  localModelPath,
  modelRepoInputRef,
  modelSource,
  selectedPreset,
  sliderValue,
  onClearTestState,
  onCustomModelFileChange,
  onCustomModelRepoChange,
  onGpuLayersChange,
  onGpuLayersInputChange,
  onLocalMmprojPathChange,
  onLocalModelPathChange,
  onModelSourceChange,
  onSelectedPresetChange,
  onSubmit
}: GemmaModelSettingsSectionProps): React.JSX.Element {
  return (
    <>
      <div className="settings-field-stack">
        <span>모델 소스</span>
        <div className="settings-mode-group" role="tablist" aria-label="모델 소스">
          {MODEL_SOURCE_OPTIONS.map((option) => (
            <button
              key={option.id}
              type="button"
              className={`settings-preset-button ${modelSource === option.id ? "active" : ""}`}
              onClick={() => {
                onClearTestState();
                onModelSourceChange(option.id);
              }}
              disabled={controlsBusy}
              aria-pressed={modelSource === option.id}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p className="muted-line modal-note">
          {MODEL_SOURCE_OPTIONS.find((option) => option.id === modelSource)?.description}
        </p>
      </div>

      {modelSource === "huggingface" ? (
        <>
          <div className="settings-field-stack">
            <span>모델</span>
            <div className="settings-preset-group" role="tablist" aria-label="모델 프리셋">
              {(["q3", "q4", "q6", "custom"] as const).map((presetId) => (
                <button
                  key={presetId}
                  type="button"
                  className={`settings-preset-button ${selectedPreset === presetId ? "active" : ""}`}
                  onClick={() => {
                    onClearTestState();
                    onSelectedPresetChange(presetId);
                  }}
                  disabled={controlsBusy}
                  aria-pressed={selectedPreset === presetId}
                >
                  {presetId === "custom" ? "커스텀" : MODEL_PRESETS[presetId].label}
                </button>
              ))}
            </div>
            <p className="muted-line modal-note">대략 권장 VRAM: Q3 약 16GB, Q4 약 24GB, Q6 약 32GB</p>
          </div>
          {selectedPreset === "custom" ? (
            <>
              <label>
                HF repo
                <input
                  ref={modelRepoInputRef}
                  value={customModelRepo}
                  disabled={controlsBusy}
                  onChange={(event) => {
                    onClearTestState();
                    onCustomModelRepoChange(event.target.value);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      onSubmit();
                    }
                  }}
                />
              </label>
              <label>
                GGUF 파일명
                <input
                  value={customModelFile}
                  disabled={controlsBusy}
                  onChange={(event) => {
                    onClearTestState();
                    onCustomModelFileChange(event.target.value);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      onSubmit();
                    }
                  }}
                />
              </label>
            </>
          ) : null}
        </>
      ) : (
        <>
          <div className="settings-field-stack">
            <span>로컬 모델 파일</span>
            <div className="settings-file-row">
              <input
                ref={localModelInputRef}
                value={localModelPath}
                disabled={controlsBusy}
                onChange={(event) => {
                  onClearTestState();
                  onLocalModelPathChange(event.target.value);
                }}
                placeholder="C:\\models\\my-model.gguf"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    onSubmit();
                  }
                }}
              />
            </div>
          </div>

          <div className="settings-field-stack">
            <span>mmproj 파일</span>
            <div className="settings-file-row">
              <input
                value={localMmprojPath}
                disabled={controlsBusy}
                onChange={(event) => {
                  onClearTestState();
                  onLocalMmprojPathChange(event.target.value);
                }}
                placeholder="같은 폴더면 자동 탐지, 필요하면 직접 지정"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    onSubmit();
                  }
                }}
              />
            </div>
            <p className="muted-line modal-note">
              mmproj는 같은 폴더에서 자동으로 찾아보고, 안 잡히면 직접 지정할 수 있습니다.
            </p>
          </div>
        </>
      )}

      <div className="settings-field-stack">
        <span>GPU layers</span>
        <div className="settings-gpu-row">
          <input
            ref={gpuSliderRef}
            className="settings-gpu-slider"
            type="range"
            min={0}
            max={MAX_GPU_LAYERS}
            step={1}
            value={sliderValue}
            style={rangeProgressStyle(sliderValue, 0, MAX_GPU_LAYERS)}
            disabled={controlsBusy}
            onChange={(event) => {
              onClearTestState();
              onGpuLayersChange(String(clampGpuLayers(Number(event.target.value))));
            }}
          />
          <input
            className="settings-gpu-input"
            type="number"
            min={0}
            max={MAX_GPU_LAYERS}
            step={1}
            value={gpuLayers}
            disabled={controlsBusy}
            onChange={onGpuLayersInputChange}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                onSubmit();
              }
            }}
          />
        </div>
        <p className="muted-line modal-note">0부터 30까지 설정할 수 있습니다.</p>
      </div>
    </>
  );
}
