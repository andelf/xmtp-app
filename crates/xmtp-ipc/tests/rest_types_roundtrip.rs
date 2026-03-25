use xmtp_core::{ConnectionState, DaemonState};
use xmtp_ipc::{
    ApiErrorBody, ApiErrorDetail, ConversationUpdatedEvent, DaemonEventData, DaemonEventEnvelope,
    EmojiRequest, GroupCreateRequest, HistoryItem, LoginRequest, RecipientMessageRequest,
    RecipientRequest, SendMessageRequest, StatusResponse,
};

#[test]
fn login_request_roundtrips_as_json() {
    let request = LoginRequest {
        env: "dev".to_owned(),
        api_url: Some("https://grpc.dev.xmtp.network:443".to_owned()),
    };

    let json = serde_json::to_string(&request).expect("serialize login request");
    let decoded: LoginRequest = serde_json::from_str(&json).expect("deserialize login request");

    assert_eq!(decoded.env, "dev");
    assert_eq!(decoded.api_url.as_deref(), Some("https://grpc.dev.xmtp.network:443"));
}

#[test]
fn direct_message_requests_roundtrip_as_json() {
    let open = RecipientRequest {
        recipient: "inbox-123".to_owned(),
    };
    let send = RecipientMessageRequest {
        recipient: "inbox-123".to_owned(),
        message: "hello".to_owned(),
    };

    let open_json = serde_json::to_string(&open).expect("serialize open request");
    let send_json = serde_json::to_string(&send).expect("serialize send request");

    let decoded_open: RecipientRequest =
        serde_json::from_str(&open_json).expect("deserialize open request");
    let decoded_send: RecipientMessageRequest =
        serde_json::from_str(&send_json).expect("deserialize send request");

    assert_eq!(decoded_open.recipient, "inbox-123");
    assert_eq!(decoded_send.recipient, "inbox-123");
    assert_eq!(decoded_send.message, "hello");
}

#[test]
fn group_and_message_requests_roundtrip_as_json() {
    let group = GroupCreateRequest {
        name: Some("team".to_owned()),
        members: vec!["member-1".to_owned(), "member-2".to_owned()],
    };
    let send = SendMessageRequest {
        message: "hi".to_owned(),
    };
    let emoji = EmojiRequest {
        emoji: "👍".to_owned(),
    };

    let group_json = serde_json::to_string(&group).expect("serialize group request");
    let send_json = serde_json::to_string(&send).expect("serialize send request");
    let emoji_json = serde_json::to_string(&emoji).expect("serialize emoji request");

    let decoded_group: GroupCreateRequest =
        serde_json::from_str(&group_json).expect("deserialize group request");
    let decoded_send: SendMessageRequest =
        serde_json::from_str(&send_json).expect("deserialize send request");
    let decoded_emoji: EmojiRequest =
        serde_json::from_str(&emoji_json).expect("deserialize emoji request");

    assert_eq!(decoded_group.name.as_deref(), Some("team"));
    assert_eq!(decoded_group.members.len(), 2);
    assert_eq!(decoded_send.message, "hi");
    assert_eq!(decoded_emoji.emoji, "👍");
}

#[test]
fn daemon_event_envelope_roundtrips_as_json() {
    let event = DaemonEventEnvelope {
        event_id: "evt-1".to_owned(),
        payload: DaemonEventData::Status(StatusResponse {
            daemon_state: DaemonState::Running,
            connection_state: ConnectionState::Connected,
            inbox_id: Some("inbox-123".to_owned()),
            installation_id: Some("install-123".to_owned()),
        }),
    };

    let json = serde_json::to_string(&event).expect("serialize daemon event");
    let decoded: DaemonEventEnvelope =
        serde_json::from_str(&json).expect("deserialize daemon event");

    assert_eq!(decoded.event_id, "evt-1");
    match decoded.payload {
        DaemonEventData::Status(status) => {
            assert!(matches!(status.daemon_state, DaemonState::Running));
            assert!(matches!(status.connection_state, ConnectionState::Connected));
            assert_eq!(status.inbox_id.as_deref(), Some("inbox-123"));
        }
        _ => panic!("unexpected event payload"),
    }
}

#[test]
fn history_event_roundtrips_as_json() {
    let event = DaemonEventEnvelope {
        event_id: "evt-2".to_owned(),
        payload: DaemonEventData::HistoryItem {
            conversation_id: "conv-1".to_owned(),
            item: HistoryItem {
                message_id: "msg-1".to_owned(),
                sender_inbox_id: "sender-1".to_owned(),
                sent_at_ns: 1,
                content_kind: "text".to_owned(),
                content: "hello".to_owned(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
        },
    };

    let json = serde_json::to_string(&event).expect("serialize history event");
    let decoded: DaemonEventEnvelope =
        serde_json::from_str(&json).expect("deserialize history event");

    match decoded.payload {
        DaemonEventData::HistoryItem {
            conversation_id,
            item,
        } => {
            assert_eq!(conversation_id, "conv-1");
            assert_eq!(item.message_id, "msg-1");
            assert_eq!(item.content, "hello");
        }
        _ => panic!("unexpected history payload"),
    }
}

#[test]
fn conversation_updated_event_roundtrips_as_json() {
    let event = DaemonEventEnvelope {
        event_id: "evt-3".to_owned(),
        payload: DaemonEventData::ConversationUpdated(ConversationUpdatedEvent {
            conversation_id: "conv-1".to_owned(),
            name: Some("renamed".to_owned()),
            member_count: 3,
        }),
    };

    let json = serde_json::to_string(&event).expect("serialize conversation updated event");
    let decoded: DaemonEventEnvelope =
        serde_json::from_str(&json).expect("deserialize conversation updated event");

    match decoded.payload {
        DaemonEventData::ConversationUpdated(update) => {
            assert_eq!(update.conversation_id, "conv-1");
            assert_eq!(update.name.as_deref(), Some("renamed"));
            assert_eq!(update.member_count, 3);
        }
        _ => panic!("unexpected conversation updated payload"),
    }
}

#[test]
fn api_error_body_roundtrips_as_json() {
    let body = ApiErrorBody {
        error: ApiErrorDetail {
            code: "unsupported_operation".to_owned(),
            message: "Leave group is not supported in this version".to_owned(),
        },
    };

    let json = serde_json::to_string(&body).expect("serialize api error body");
    let decoded: ApiErrorBody = serde_json::from_str(&json).expect("deserialize api error body");

    assert_eq!(decoded.error.code, "unsupported_operation");
    assert_eq!(
        decoded.error.message,
        "Leave group is not supported in this version"
    );
}
