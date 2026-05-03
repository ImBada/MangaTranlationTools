import { describe, expect, it } from "vitest";
import {
  buildBaseTranslationOptions,
  DEFAULT_CODEX_MODEL,
  DEFAULT_CODEX_OAUTH_PORT,
  DEFAULT_CODEX_REASONING_EFFORT,
  DEFAULT_TRANSLATION_MODE,
  parseStoredAppSettings,
  resolveDefaultAppSettings
} from "../src/server/appSettings";
import type { AppSettings } from "../src/shared/types";

describe("app settings helpers", () => {
  it("uses OpenAI Codex as the built-in default provider", () => {
    const defaults = resolveDefaultAppSettings();

    expect(defaults.modelProvider).toBe("openai-codex");
    expect(defaults.codex.model).toBe(DEFAULT_CODEX_MODEL);
    expect(defaults.codex.reasoningEffort).toBe(DEFAULT_CODEX_REASONING_EFFORT);
    expect(defaults.codex.oauthPort).toBe(DEFAULT_CODEX_OAUTH_PORT);
    expect(defaults.translationMode).toBe(DEFAULT_TRANSLATION_MODE);
    expect(defaults.translationParallel).toEqual({ enabled: false, maxConcurrency: 2 });
  });

  it("fills missing or partial stored settings from environment-based defaults", () => {
    const env = {
      MANGA_TRANSLATOR_CODEX_MODEL: "env-codex",
      MANGA_TRANSLATOR_CODEX_REASONING_EFFORT: "high",
      MANGA_TRANSLATOR_CODEX_OAUTH_PORT: "10535",
      MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL: " http://localhost:1234/v1/ ",
      MANGA_TRANSLATOR_OPENAI_COMPATIBLE_API_KEY: " env-key ",
      MANGA_TRANSLATOR_OPENAI_COMPATIBLE_MODEL: "env-compatible"
    } satisfies NodeJS.ProcessEnv;
    const defaults = resolveDefaultAppSettings(env);

    expect(parseStoredAppSettings("", defaults)).toEqual(defaults);
    expect(parseStoredAppSettings("{\"codex\":{\"model\":\"saved-codex\"}}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: {
        model: "saved-codex",
        reasoningEffort: "high",
        oauthPort: 10535
      },
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });
  });

  it("normalizes nsfw mode from stored settings", () => {
    const defaults = resolveDefaultAppSettings();

    expect(parseStoredAppSettings("{\"nsfwMode\":true}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: defaults.codex,
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: true
    });

    expect(parseStoredAppSettings("{\"nsfwMode\":\"off\"}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: defaults.codex,
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });
  });

  it("normalizes AI translation parallel settings from stored settings", () => {
    const defaults = resolveDefaultAppSettings();

    expect(parseStoredAppSettings("{\"translationParallel\":{\"enabled\":true,\"maxConcurrency\":4}}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: defaults.codex,
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: {
        enabled: true,
        maxConcurrency: 4
      },
      nsfwMode: false
    });

    expect(parseStoredAppSettings("{\"translationParallel\":{\"enabled\":\"off\",\"maxConcurrency\":99}}", defaults).translationParallel).toEqual({
      enabled: false,
      maxConcurrency: 8
    });
  });

  it("fills invalid or missing translation mode with the default", () => {
    const defaults = resolveDefaultAppSettings();

    expect(parseStoredAppSettings("{\"translationMode\":\"accuracy\"}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: defaults.codex,
      openAICompatible: defaults.openAICompatible,
      translationMode: "accuracy",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });

    expect(parseStoredAppSettings("{\"translationMode\":\"turbo\"}", defaults)).toEqual({
      modelProvider: defaults.modelProvider,
      codex: defaults.codex,
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });
  });

  it("builds fast mode translation options from saved model settings while preserving other defaults", () => {
    const settings: AppSettings = {
      modelProvider: "openai-compatible",
      codex: {
        model: DEFAULT_CODEX_MODEL,
        reasoningEffort: DEFAULT_CODEX_REASONING_EFFORT,
        oauthPort: DEFAULT_CODEX_OAUTH_PORT
      },
      openAICompatible: {
        baseUrl: "http://localhost:1234/v1",
        apiKey: "local-key",
        model: "saved-compatible"
      },
      translationMode: "fast",
      translationParallel: {
        enabled: false,
        maxConcurrency: 2
      },
      nsfwMode: true
    };

    const options = buildBaseTranslationOptions({
      jobId: "job-1",
      runDir: "C:/runs/job-1",
      settings,
      env: {
        MANGA_TRANSLATOR_TEMPERATURE: "0.2"
      } satisfies NodeJS.ProcessEnv
    });

    expect(options.modelProvider).toBe("openai-compatible");
    expect(options.openAICompatibleBaseUrl).toBe("http://localhost:1234/v1");
    expect(options.openAICompatibleApiKey).toBe("local-key");
    expect(options.openAICompatibleModel).toBe("saved-compatible");
    expect(options.codexModel).toBe(DEFAULT_CODEX_MODEL);
    expect(options.codexReasoningEffort).toBe(DEFAULT_CODEX_REASONING_EFFORT);
    expect(options.codexOauthPort).toBe(DEFAULT_CODEX_OAUTH_PORT);
    expect(options.nsfwMode).toBe(true);
    expect(options.temperature).toBe(0.2);
    expect(options.maxTokens).toBe(900);
    expect(options.imageMinTokens).toBe(640);
    expect(options.imageMaxTokens).toBe(640);
    expect(options.includeEnhancedVariant).toBe(false);
    expect(options.topP).toBe(0.85);
    expect(options.outputDir).toBe("C:/runs/job-1");
    expect(options.label).toBe("app-job-1");
  });

  it("builds accuracy mode translation options with the previous larger image budget", () => {
    const settings: AppSettings = {
      ...resolveDefaultAppSettings(),
      translationMode: "accuracy"
    };

    const options = buildBaseTranslationOptions({
      jobId: "job-2",
      runDir: "C:/runs/job-2",
      settings
    });

    expect(options.maxTokens).toBe(1400);
    expect(options.imageMinTokens).toBe(1120);
    expect(options.imageMaxTokens).toBe(1120);
    expect(options.includeEnhancedVariant).toBe(true);
  });

  it("maps unknown legacy provider settings back to the default provider", () => {
    const defaults = resolveDefaultAppSettings();

    expect(
      parseStoredAppSettings(
        JSON.stringify({
          modelProvider: "legacy-local",
          legacyLocal: { model: "ignored" }
        }),
        defaults
      )
    ).toEqual(defaults);
  });

  it("normalizes Codex provider settings", () => {
    const defaults = resolveDefaultAppSettings();

    expect(
      parseStoredAppSettings(
        JSON.stringify({
          modelProvider: "openai-codex",
          codex: {
            model: "gpt-5.5",
            reasoningEffort: "xhigh",
            oauthPort: 10532
          }
        }),
        defaults
      )
    ).toEqual({
      modelProvider: "openai-codex",
      codex: {
        model: "gpt-5.5",
        reasoningEffort: "xhigh",
        oauthPort: 10532
      },
      openAICompatible: defaults.openAICompatible,
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });
  });

  it("maps the old Codex minimal value to low", () => {
    const defaults = resolveDefaultAppSettings();

    expect(
      parseStoredAppSettings(
        JSON.stringify({
          modelProvider: "openai-codex",
          codex: {
            reasoningEffort: "minimal"
          }
        }),
        defaults
      ).codex.reasoningEffort
    ).toBe("low");
  });

  it("normalizes custom OpenAI-compatible provider settings", () => {
    const defaults = resolveDefaultAppSettings();

    expect(
      parseStoredAppSettings(
        JSON.stringify({
          modelProvider: "openai-compatible",
          openAICompatible: {
            baseUrl: " http://localhost:1234/v1/ ",
            apiKey: " local-key ",
            model: "local-vision-model"
          }
        }),
        defaults
      )
    ).toEqual({
      modelProvider: "openai-compatible",
      codex: defaults.codex,
      openAICompatible: {
        baseUrl: "http://localhost:1234/v1",
        apiKey: "local-key",
        model: "local-vision-model"
      },
      translationMode: "fast",
      translationParallel: defaults.translationParallel,
      nsfwMode: false
    });
  });
});
