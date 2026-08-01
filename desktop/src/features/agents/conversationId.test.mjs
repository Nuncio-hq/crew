import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { deriveAgentConversationId } from "./conversationId.ts";

describe("deriveAgentConversationId", () => {
  it("matches Rust conversation identity vectors", () => {
    assert.equal(
      deriveAgentConversationId(
        "00112233-4455-6677-8899-aabbccddeeff",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      ),
      "7415ce56-7adc-d430-f133-c5e06a8e5113",
    );
    assert.equal(
      deriveAgentConversationId(
        "11111111-2222-3333-4444-555555555555",
        "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd",
      ),
      "026dfba8-bd95-7847-6709-920a0e6d9b97",
    );
  });
});
