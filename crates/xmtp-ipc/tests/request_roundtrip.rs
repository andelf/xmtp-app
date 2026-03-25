use xmtp_core::{ConnectionState, DaemonState};
use xmtp_ipc::{DaemonRequest, DaemonResponse, DaemonResponseData, IpcEnvelope, StatusResponse};

#[test]
fn get_status_request_roundtrips_as_json() {
    let request = IpcEnvelope {
        version: 1,
        request_id: "req-1".to_owned(),
        payload: DaemonRequest::GetStatus,
    };

    let json = serde_json::to_string(&request).expect("serialize request");
    let decoded: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&json).expect("deserialize request");

    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.request_id, "req-1");
    assert!(matches!(decoded.payload, DaemonRequest::GetStatus));
}

#[test]
fn status_response_roundtrips_as_json() {
    let response = IpcEnvelope {
        version: 1,
        request_id: "req-2".to_owned(),
        payload: DaemonResponse {
            ok: true,
            result: Some(DaemonResponseData::Status(StatusResponse {
                daemon_state: DaemonState::Running,
                connection_state: ConnectionState::Connected,
                inbox_id: Some("inbox-123".to_owned()),
                installation_id: Some("install-123".to_owned()),
            })),
            error: None,
        },
    };

    let json = serde_json::to_string(&response).expect("serialize response");
    let decoded: IpcEnvelope<DaemonResponse> =
        serde_json::from_str(&json).expect("deserialize response");

    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.request_id, "req-2");
    let result = decoded.payload.result.expect("status response");
    match result {
        DaemonResponseData::Status(status) => {
            assert!(matches!(status.daemon_state, DaemonState::Running));
            assert!(matches!(status.connection_state, ConnectionState::Connected));
            assert_eq!(status.inbox_id.as_deref(), Some("inbox-123"));
        }
        _ => panic!("unexpected response type"),
    }
}

#[test]
fn reply_and_react_requests_roundtrip_as_json() {
    let reply = IpcEnvelope {
        version: 1,
        request_id: "req-3".to_owned(),
        payload: DaemonRequest::Reply {
            message_id: "msg-1".to_owned(),
            message: "hello".to_owned(),
        },
    };
    let react = IpcEnvelope {
        version: 1,
        request_id: "req-4".to_owned(),
        payload: DaemonRequest::React {
            message_id: "msg-2".to_owned(),
            emoji: "👍".to_owned(),
        },
    };

    let reply_json = serde_json::to_string(&reply).expect("serialize reply");
    let react_json = serde_json::to_string(&react).expect("serialize react");

    let decoded_reply: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&reply_json).expect("deserialize reply");
    let decoded_react: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&react_json).expect("deserialize react");

    match decoded_reply.payload {
        DaemonRequest::Reply { message_id, message } => {
            assert_eq!(message_id, "msg-1");
            assert_eq!(message, "hello");
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_react.payload {
        DaemonRequest::React { message_id, emoji } => {
            assert_eq!(message_id, "msg-2");
            assert_eq!(emoji, "👍");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn group_requests_roundtrip_as_json() {
    let create_group = IpcEnvelope {
        version: 1,
        request_id: "req-5".to_owned(),
        payload: DaemonRequest::CreateGroup {
            name: Some("team".to_owned()),
            members: vec!["member-1".to_owned(), "member-2".to_owned()],
        },
    };
    let send_group = IpcEnvelope {
        version: 1,
        request_id: "req-6".to_owned(),
        payload: DaemonRequest::SendGroup {
            conversation_id: "conv-1".to_owned(),
            message: "hello-group".to_owned(),
        },
    };

    let create_json = serde_json::to_string(&create_group).expect("serialize create group");
    let send_json = serde_json::to_string(&send_group).expect("serialize send group");

    let decoded_create: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&create_json).expect("deserialize create group");
    let decoded_send: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&send_json).expect("deserialize send group");

    match decoded_create.payload {
        DaemonRequest::CreateGroup { name, members } => {
            assert_eq!(name.as_deref(), Some("team"));
            assert_eq!(members, vec!["member-1", "member-2"]);
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_send.payload {
        DaemonRequest::SendGroup {
            conversation_id,
            message,
        } => {
            assert_eq!(conversation_id, "conv-1");
            assert_eq!(message, "hello-group");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn list_and_group_members_requests_roundtrip_as_json() {
    let list = IpcEnvelope {
        version: 1,
        request_id: "req-7".to_owned(),
        payload: DaemonRequest::ListConversations {
            kind: Some("group".to_owned()),
        },
    };
    let members = IpcEnvelope {
        version: 1,
        request_id: "req-8".to_owned(),
        payload: DaemonRequest::GroupMembers {
            conversation_id: "conv-2".to_owned(),
        },
    };

    let list_json = serde_json::to_string(&list).expect("serialize list");
    let members_json = serde_json::to_string(&members).expect("serialize members");

    let decoded_list: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&list_json).expect("deserialize list");
    let decoded_members: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&members_json).expect("deserialize members");

    match decoded_list.payload {
        DaemonRequest::ListConversations { kind } => {
            assert_eq!(kind.as_deref(), Some("group"));
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_members.payload {
        DaemonRequest::GroupMembers { conversation_id } => {
            assert_eq!(conversation_id, "conv-2");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn group_management_requests_roundtrip_as_json() {
    let rename = IpcEnvelope {
        version: 1,
        request_id: "req-9".to_owned(),
        payload: DaemonRequest::RenameGroup {
            conversation_id: "conv-3".to_owned(),
            name: "renamed".to_owned(),
        },
    };
    let add = IpcEnvelope {
        version: 1,
        request_id: "req-10".to_owned(),
        payload: DaemonRequest::AddGroupMembers {
            conversation_id: "conv-3".to_owned(),
            members: vec!["member-3".to_owned()],
        },
    };
    let remove = IpcEnvelope {
        version: 1,
        request_id: "req-11".to_owned(),
        payload: DaemonRequest::RemoveGroupMembers {
            conversation_id: "conv-3".to_owned(),
            members: vec!["member-4".to_owned()],
        },
    };
    let info = IpcEnvelope {
        version: 1,
        request_id: "req-12".to_owned(),
        payload: DaemonRequest::GroupInfo {
            conversation_id: "conv-3".to_owned(),
        },
    };

    let decoded_rename: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&rename).expect("serialize rename"))
            .expect("deserialize rename");
    let decoded_add: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&add).expect("serialize add"))
            .expect("deserialize add");
    let decoded_remove: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&remove).expect("serialize remove"))
            .expect("deserialize remove");
    let decoded_info: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&info).expect("serialize info"))
            .expect("deserialize info");

    match decoded_rename.payload {
        DaemonRequest::RenameGroup {
            conversation_id,
            name,
        } => {
            assert_eq!(conversation_id, "conv-3");
            assert_eq!(name, "renamed");
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_add.payload {
        DaemonRequest::AddGroupMembers {
            conversation_id,
            members,
        } => {
            assert_eq!(conversation_id, "conv-3");
            assert_eq!(members, vec!["member-3"]);
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_remove.payload {
        DaemonRequest::RemoveGroupMembers {
            conversation_id,
            members,
        } => {
            assert_eq!(conversation_id, "conv-3");
            assert_eq!(members, vec!["member-4"]);
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_info.payload {
        DaemonRequest::GroupInfo { conversation_id } => {
            assert_eq!(conversation_id, "conv-3");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn info_and_unreact_requests_roundtrip_as_json() {
    let unreact = IpcEnvelope {
        version: 1,
        request_id: "req-13".to_owned(),
        payload: DaemonRequest::Unreact {
            message_id: "msg-3".to_owned(),
            emoji: "👍".to_owned(),
        },
    };
    let conversation_info = IpcEnvelope {
        version: 1,
        request_id: "req-14".to_owned(),
        payload: DaemonRequest::ConversationInfo {
            conversation_id: "conv-4".to_owned(),
        },
    };
    let message_info = IpcEnvelope {
        version: 1,
        request_id: "req-15".to_owned(),
        payload: DaemonRequest::MessageInfo {
            message_id: "msg-4".to_owned(),
        },
    };

    let decoded_unreact: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&unreact).expect("serialize unreact"))
            .expect("deserialize unreact");
    let decoded_conversation: IpcEnvelope<DaemonRequest> = serde_json::from_str(
        &serde_json::to_string(&conversation_info).expect("serialize conversation info"),
    )
    .expect("deserialize conversation info");
    let decoded_message: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&message_info).expect("serialize message info"))
            .expect("deserialize message info");

    match decoded_unreact.payload {
        DaemonRequest::Unreact { message_id, emoji } => {
            assert_eq!(message_id, "msg-3");
            assert_eq!(emoji, "👍");
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_conversation.payload {
        DaemonRequest::ConversationInfo { conversation_id } => {
            assert_eq!(conversation_id, "conv-4");
        }
        _ => panic!("unexpected request type"),
    }

    match decoded_message.payload {
        DaemonRequest::MessageInfo { message_id } => {
            assert_eq!(message_id, "msg-4");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn leave_request_roundtrips_as_json() {
    let leave = IpcEnvelope {
        version: 1,
        request_id: "req-16".to_owned(),
        payload: DaemonRequest::LeaveConversation {
            conversation_id: "conv-5".to_owned(),
        },
    };

    let decoded: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&leave).expect("serialize leave"))
            .expect("deserialize leave");

    match decoded.payload {
        DaemonRequest::LeaveConversation { conversation_id } => {
            assert_eq!(conversation_id, "conv-5");
        }
        _ => panic!("unexpected request type"),
    }
}

#[test]
fn watch_history_request_roundtrips_as_json() {
    let watch = IpcEnvelope {
        version: 1,
        request_id: "req-17".to_owned(),
        payload: DaemonRequest::WatchHistory {
            conversation_id: "conv-6".to_owned(),
        },
    };

    let decoded: IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&serde_json::to_string(&watch).expect("serialize watch"))
            .expect("deserialize watch");

    match decoded.payload {
        DaemonRequest::WatchHistory { conversation_id } => {
            assert_eq!(conversation_id, "conv-6");
        }
        _ => panic!("unexpected request type"),
    }
}
