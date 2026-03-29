import {
  decodedToMessageItem,
  type DecodedMessageLike,
} from "../utils/messageDecoder";

const CONV_ID = "test-conv";
const MY_INBOX = "my-inbox-id";
const OTHER_INBOX = "other-inbox-id";

/** Helper to build a minimal DecodedMessage-like object with nativeContent. */
function fakeMsg(
  nativeContent: Record<string, any>,
  overrides: Partial<Record<string, any>> = {}
): DecodedMessageLike {
  return {
    id: overrides.id ?? "msg-1",
    senderInboxId: overrides.senderInboxId ?? OTHER_INBOX,
    sentNs: overrides.sentNs ?? Date.now() * 1_000_000,
    deliveryStatus: overrides.deliveryStatus ?? "published",
    contentTypeId: overrides.contentTypeId ?? "xmtp.org/text:1.0",
    nativeContent,
    // fields we don't use but exist on real messages
    topic: "topic",
    content: () => {
      throw new Error("should not call content()");
    },
    fallback: overrides.fallback,
  } as unknown as DecodedMessageLike;
}

describe("decodedToMessageItem", () => {
  // ---- Plain text ----
  it("converts a plain text message", () => {
    const msg = fakeMsg({ text: "hello world" });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item).not.toBeNull();
    expect(item!.text).toBe("hello world");
    expect(item!.isOwn).toBe(false);
    expect(item!.conversationId).toBe(CONV_ID);
  });

  it("marks own messages correctly", () => {
    const msg = fakeMsg({ text: "my msg" }, { senderInboxId: MY_INBOX });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.isOwn).toBe(true);
  });

  it("converts numeric text to string", () => {
    const msg = fakeMsg({ text: 42 });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("42");
  });

  // ---- Reply ----
  it("converts a reply message", () => {
    const msg = fakeMsg({
      reply: {
        reference: "original-msg-id",
        content: { text: "reply text" },
        contentType: "xmtp.org/text:1.0",
      },
    });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item).not.toBeNull();
    expect(item!.text).toBe("reply text");
    expect(item!.replyRef).toEqual({
      referenceMessageId: "original-msg-id",
      referenceText: undefined,
    });
  });

  it("falls back to [reply] when reply has no text", () => {
    const msg = fakeMsg({
      reply: { reference: "ref-1", content: {} },
    });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("[reply]");
  });

  // ---- Filtered types return null ----
  it("returns null for reaction", () => {
    const msg = fakeMsg({ reaction: { content: "👍", action: "added" } });
    expect(decodedToMessageItem(msg, CONV_ID, MY_INBOX)).toBeNull();
  });

  it("returns null for reactionV2", () => {
    const msg = fakeMsg({ reactionV2: { content: "❤️", action: "added" } });
    expect(decodedToMessageItem(msg, CONV_ID, MY_INBOX)).toBeNull();
  });

  it("returns null for read receipt", () => {
    const msg = fakeMsg({ readReceipt: {} });
    expect(decodedToMessageItem(msg, CONV_ID, MY_INBOX)).toBeNull();
  });

  it("returns null for group update", () => {
    const msg = fakeMsg({ groupUpdated: { members: [] } });
    expect(decodedToMessageItem(msg, CONV_ID, MY_INBOX)).toBeNull();
  });

  // ---- Unknown content type ----
  it("extracts text from unknown content type", () => {
    const msg = fakeMsg(
      {
        unknown: {
          contentTypeId: "xmtp.org/markdown:1.0",
          content: "# Hello",
        },
      },
      { contentTypeId: "xmtp.org/markdown:1.0" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("# Hello");
  });

  it("shows unsupported message for unknown type without content", () => {
    const msg = fakeMsg(
      { unknown: { contentTypeId: "xmtp.org/transaction:1.0" } },
      { contentTypeId: "xmtp.org/transaction:1.0" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item).not.toBeNull();
    expect(item!.text).toBe(
      "Unsupported content type: xmtp.org/transaction:1.0"
    );
  });

  it("uses msg.fallback for unknown type", () => {
    const msg = fakeMsg(
      { unknown: { contentTypeId: "xmtp.org/actions:1.0" } },
      { fallback: "This is a fallback" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("This is a fallback");
  });

  // ---- Encoded payload ----
  it("decodes base64 encoded content", () => {
    const content = globalThis.Buffer.from("encoded text").toString("base64");
    const msg = fakeMsg(
      { encoded: JSON.stringify({ content }) },
      { contentTypeId: "xmtp.org/custom:1.0" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("encoded text");
  });

  it("uses fallback from encoded payload", () => {
    const msg = fakeMsg(
      { encoded: JSON.stringify({ fallback: "fallback text" }) },
      { contentTypeId: "xmtp.org/custom:1.0" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item!.text).toBe("fallback text");
  });

  it("shows unsupported for encoded with no extractable content", () => {
    const msg = fakeMsg(
      { encoded: JSON.stringify({}) },
      { contentTypeId: "xmtp.org/binary:1.0" }
    );
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item).not.toBeNull();
    expect(item!.text).toBe("Unsupported content type: xmtp.org/binary:1.0");
  });

  // ---- Edge cases ----
  it("returns null when nativeContent is missing", () => {
    const msg = { id: "msg-1", senderInboxId: "x" } as unknown as DecodedMessageLike;
    expect(decodedToMessageItem(msg, CONV_ID, MY_INBOX)).toBeNull();
  });

  it("shows unsupported for completely empty nativeContent", () => {
    const msg = fakeMsg({}, { contentTypeId: "xmtp.org/mystery:1.0" });
    const item = decodedToMessageItem(msg, CONV_ID, MY_INBOX);

    expect(item).not.toBeNull();
    expect(item!.text).toBe("Unsupported content type: xmtp.org/mystery:1.0");
  });
});
