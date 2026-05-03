import { describe, expect, it } from "vitest";

const runtimeHelpers = require("../src/server/runtime/simple-page-translate.cjs") as {
  buildChatRequestBody: (options: { [key: string]: unknown }, messages: unknown[], maxTokens?: number) => {
    model: string;
    reasoning_budget?: number;
    reasoning_effort?: string;
    enable_thinking?: boolean;
    think?: boolean;
    top_k?: number;
  };
  buildResponsesRequestBody: (options: { [key: string]: unknown }, imageVariants: Array<{ role: string; dataUrl: string }>) => {
    model: string;
    instructions: string;
    input: Array<{ role: string; content: Array<{ type: string; text?: string; image_url?: string }> }>;
    reasoning: { effort: string };
    stream: boolean;
    store: boolean;
  };
  extractModelOutputText: (parsed: unknown) => string;
  parseResponsesSseText: (rawText: string) => { outputText: string; eventCount: number; rawResponse: unknown };
};
const {
  buildChatRequestBody,
  buildResponsesRequestBody,
  extractModelOutputText,
  parseResponsesSseText
} = runtimeHelpers;

describe("runtime model request helpers", () => {
  it("disables thinking for Ollama-compatible chat completion endpoints", () => {
    const requestBody = buildChatRequestBody(
      {
        modelProvider: "openai-compatible",
        openAICompatibleBaseUrl: "https://ollama.com/v1",
        openAICompatibleModel: "kimi-k2.6",
        temperature: 0,
        topP: 0.85,
        topK: 40
      },
      []
    );

    expect(requestBody).toMatchObject({
      model: "kimi-k2.6",
      reasoning_budget: 0,
      reasoning_effort: "none",
      enable_thinking: false,
      think: false
    });
    expect(requestBody.top_k).toBeUndefined();
    expect(requestBody.reasoning_effort).toBe("none");
  });

  it("keeps generic OpenAI-compatible chat completion requests spec-shaped", () => {
    const requestBody = buildChatRequestBody(
      {
        modelProvider: "openai-compatible",
        openAICompatibleBaseUrl: "https://api.example.test/v1",
        openAICompatibleModel: "vision-model",
        temperature: 0,
        topP: 0.85,
        topK: 40
      },
      []
    );

    expect(requestBody.reasoning_budget).toBeUndefined();
    expect(requestBody.reasoning_effort).toBeUndefined();
    expect(requestBody.enable_thinking).toBeUndefined();
    expect(requestBody.think).toBeUndefined();
    expect(requestBody.top_k).toBeUndefined();
  });

  it("builds Codex Responses requests with input_image data URLs", () => {
    const requestBody = buildResponsesRequestBody(
      {
        modelProvider: "openai-codex",
        codexModel: "gpt-5.5",
        codexReasoningEffort: "xhigh"
      },
      [{ role: "original", dataUrl: "data:image/png;base64,abc123" }]
    );

    expect(requestBody.model).toBe("gpt-5.5");
    expect(requestBody.reasoning.effort).toBe("xhigh");
    expect(requestBody.stream).toBe(true);
    expect(requestBody.store).toBe(false);
    expect(requestBody.input[0]?.content.some((part) => part.type === "input_image" && part.image_url === "data:image/png;base64,abc123")).toBe(true);
  });

  it("extracts text from Responses API output payloads", () => {
    expect(
      extractModelOutputText({
        output: [
          {
            type: "message",
            content: [
              {
                type: "output_text",
                text: "id: 1\nko: 테스트"
              }
            ]
          }
        ]
      })
    ).toBe("id: 1\nko: 테스트");
  });

  it("collects Responses API streaming text deltas", () => {
    const parsed = parseResponsesSseText(
      [
        'event: response.output_text.delta',
        'data: {"type":"response.output_text.delta","delta":"id: 1"}',
        "",
        'event: response.output_text.delta',
        'data: {"type":"response.output_text.delta","delta":"\\nko: 테스트"}',
        "",
        'event: response.completed',
        'data: {"type":"response.completed","response":{"id":"resp_1","output":[]}}',
        "",
        "data: [DONE]",
        ""
      ].join("\n")
    );

    expect(parsed.outputText).toBe("id: 1\nko: 테스트");
    expect(parsed.eventCount).toBe(3);
  });
});
