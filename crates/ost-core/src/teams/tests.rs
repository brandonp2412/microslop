use super::{
    ChatMessage, ConversationsResponse, GraphChat, GraphChatMember, GraphCollection, GraphMessage,
    GraphMessageAttachment, GraphMessageUser, GraphReaction, GraphReactionIdentitySet,
    NativeMessage, NativeMessagesResponse, audio_message_content, chat_name_or_id,
    custom_reaction_parts, display_name_or_id, graph_message, graph_message_reference_quotes,
    hosted_content_ids, ids_equal, image_src_urls, is_displayable_chat_name, is_graph_self_chat,
    message_cache_key, message_content_and_quotes, message_html, native_message,
    native_reaction_counts, native_sender_id, order_chat_messages_chronologically,
    profile_photo_user_id, reaction_counts, should_refresh_chats, strip_html,
};

#[test]
fn image_only_messages_survive_before_images_are_downloaded() {
    let native: NativeMessage = serde_json::from_value(serde_json::json!({
        "id": "self-image",
        "messagetype": "RichText/Html",
        "content": "<img src=\"https://example.test/photo.png\" />"
    }))
    .unwrap();
    let message = native_message(native, Vec::new(), None).unwrap();
    assert_eq!(message.id, "self-image");
    assert!(message.images.is_empty());
    assert_eq!(message.image_urls, ["https://example.test/photo.png"]);
    let graph: GraphMessage = serde_json::from_value(serde_json::json!({
        "id": "self-image",
        "body": {"content": "<img src=\"https://example.test/photo.png\" />"}
    }))
    .unwrap();
    assert_eq!(
        graph_message(graph, None, Vec::new()).unwrap().image_urls,
        ["https://example.test/photo.png"]
    );
}

#[test]
fn custom_reaction_parts_use_the_teams_shortcut_and_ams_document_id() {
    assert_eq!(
        custom_reaction_parts("monkey-look;0-sau-d3-d684dd9174cc3622852276ac94d31cfd"),
        Some((
            "monkey-look".to_owned(),
            "0-sau-d3-d684dd9174cc3622852276ac94d31cfd".to_owned(),
        ))
    );
    assert_eq!(custom_reaction_parts("like"), None);
    assert_eq!(custom_reaction_parts("shortcut;document;extra"), None);
}

#[test]
fn audio_message_content_uses_the_native_teams_voice_message_shape() {
    let content = audio_message_content(
        "https://au-prod.asyncgw.teams.microsoft.com/v1/objects",
        "voice-object",
    );

    assert!(content.contains("Audio.1/Message.1"));
    assert!(content.contains("au-prod.asyncgw.teams.microsoft.com/v1/objects/voice-object"));
    assert!(content.contains("/voice-object/views/thumbnail"));
    assert!(content.contains("am=voice-object"));
}

#[test]
fn message_cache_keys_are_safe_for_persistent_storage_names() {
    let key = message_cache_key(&format!("19:{}@unq.gbl.spaces", "a".repeat(400)));

    assert!(key.starts_with("message-cache-"));
    assert_eq!(key.len(), 78);
    assert!(!key.contains('/'));
    assert!(!key.contains('='));
    assert_ne!(key, message_cache_key("19:another@unq.gbl.spaces"));
}

#[test]
fn graph_chat_page_retains_its_continuation_link() {
    let page: GraphCollection<GraphChat> = serde_json::from_str(
            r#"{"value": [], "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/chats?$skiptoken=next"}"#,
        )
        .expect("the Microsoft Graph chat page should deserialize");

    assert_eq!(page.value.len(), 0);
    assert_eq!(
        page.next_link.as_deref(),
        Some("https://graph.microsoft.com/v1.0/me/chats?$skiptoken=next")
    );
}

#[test]
fn native_chat_page_retains_its_backward_continuation_link() {
    let page: ConversationsResponse = serde_json::from_str(
            r#"{"conversations":[],"_metadata":{"backwardLink":"https://amer.ng.msg.teams.microsoft.com/v1/users/me/conversations?view=msnp24Equivalent&syncState=next&pageSize=50"}}"#,
        )
        .expect("the Teams native chat page should deserialize");

    assert_eq!(
            page.metadata.and_then(|metadata| metadata.backward_link),
            Some(
                "https://amer.ng.msg.teams.microsoft.com/v1/users/me/conversations?view=msnp24Equivalent&syncState=next&pageSize=50"
                    .to_owned()
            )
        );
}

#[test]
fn native_message_page_retains_its_backward_continuation_link() {
    let page: NativeMessagesResponse = serde_json::from_str(
            r#"{"messages":[],"_metadata":{"backwardLink":"https://amer.ng.msg.teams.microsoft.com/v1/users/ME/conversations/19:chat/messages?syncState=next&pageSize=100"}}"#,
        )
        .expect("the Teams native message page should deserialize");

    assert_eq!(
            page.metadata.and_then(|metadata| metadata.backward_link),
            Some(
                "https://amer.ng.msg.teams.microsoft.com/v1/users/ME/conversations/19:chat/messages?syncState=next&pageSize=100"
                    .to_owned()
            )
        );
}

#[test]
fn graph_reference_attachments_remain_visible_as_messages() {
    let message: GraphMessage = serde_json::from_str(
            r#"{"id":"file-1","createdDateTime":"2026-08-31T00:00:00Z","body":{"contentType":"html","content":"<attachment id=\"a\"></attachment>"},"from":{"user":{"id":"ada","displayName":"Ada"}},"attachments":[{"id":"a","contentType":"reference","contentUrl":"https://example.test/file","name":"notes.pdf"}]}"#,
        )
        .expect("reference attachment should parse");

    let parsed =
        graph_message(message, None, Vec::new()).expect("file message should remain visible");
    assert_eq!(parsed.content, "📎 notes.pdf");
}

#[test]
fn message_html_preserves_multiline_messages() {
    assert_eq!(message_html("one\ntwo & three"), "one<br>two &amp; three");
}

#[test]
fn message_html_turns_web_urls_into_links() {
    assert_eq!(
        message_html("See https://example.com/a?x=1&y=2."),
        "See <a href=\"https://example.com/a?x=1&amp;y=2\">https://example.com/a?x=1&amp;y=2</a>."
    );
}

#[test]
fn message_html_preserves_fenced_code_blocks() {
    assert_eq!(
        message_html("Before\n```rust\nlet x = 1 < 2;\nhttps://example.com\n```\nAfter"),
        "Before<br><pre><code>let x = 1 &lt; 2;\nhttps://example.com</code></pre><br>After"
    );
}

#[test]
fn quoted_reply_html_is_split_from_the_reply_body() {
    let (content, quotes) = message_content_and_quotes(
        r#"<div><blockquote itemscope="" itemtype="http://schema.skype.com/Reply" itemid="1600457867820"><strong itemprop="mri" itemid="8:orgid:user">Test User</strong><span itemprop="time" itemid="1600457867820"></span><p itemprop="preview">1237 &amp; more</p></blockquote><p>this is a reply</p></div>"#,
    );

    assert_eq!(content, "this is a reply");
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].message_id.as_deref(), Some("1600457867820"));
    assert_eq!(quotes[0].sender, "Test User");
    assert_eq!(quotes[0].content, "1237 & more");
}

#[test]
fn message_reference_attachments_are_mapped_to_quotes() {
    let attachments = [GraphMessageAttachment {
            content: Some(
                r#"{"messageId":"1700000000000","messagePreview":"Hello <b>World</b>","messageSender":{"user":{"displayName":"Test User"}}}"#
                    .to_owned(),
            ),
            content_type: Some("messageReference".to_owned()),
            content_url: None,
            thumbnail_url: None,
            name: None,
        }];

    let quotes = graph_message_reference_quotes(&attachments);

    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].message_id.as_deref(), Some("1700000000000"));
    assert_eq!(quotes[0].sender, "Test User");
    assert_eq!(quotes[0].content, "Hello World");
}

#[test]
fn strip_html_preserves_block_boundaries() {
    assert_eq!(
        strip_html("<p>Hello<br>world</p><div>Next &amp; last</div>"),
        "Hello\nworld\nNext & last"
    );
}

#[test]
fn strip_html_preserves_link_destinations() {
    assert_eq!(
        strip_html(r#"See <a href="https://dev.azure.com/example/workitems/42">work item</a>"#),
        "See work item https://dev.azure.com/example/workitems/42"
    );
}

#[test]
fn strip_html_preserves_mention_markers() {
    assert_eq!(
        strip_html(r#"<p><at id="0">brandonp2412</at> please check this</p>"#),
        "@brandonp2412 please check this"
    );
}

#[test]
fn inline_emoji_render_as_text_instead_of_message_images() {
    let html = r#"<p>Hello <span><img itemtype="http://schema.skype.com/Emoji" alt="🙂" src="https://statics.teams.cdn.live.net/evergreen-assets/personal-expressions/v2/assets/emoticons/smile/default/30_f.png"></span> <emoji id="heart" alt="❤️" title="heart"></emoji><customemoji id="party" alt=":party:" source="../hostedContents/7/$value"></customemoji></p><img src="https://example.test/photo.png">"#;
    assert_eq!(strip_html(html), "Hello 🙂 ❤️:party:");
    assert_eq!(image_src_urls(html), ["https://example.test/photo.png"]);
    assert!(hosted_content_ids(html).is_empty());
}

#[test]
fn strip_html_preserves_preformatted_code_blocks() {
    assert_eq!(
        strip_html("<p>Before</p><pre><code>let x = 1 &lt; 2;\nline 2</code></pre><p>After</p>"),
        "Before\n```\nlet x = 1 < 2;\nline 2\n```\nAfter"
    );
}

#[test]
fn native_message_maps_a_direct_message_without_graph_permissions() {
    let message = native_message(
        NativeMessage {
            id: Some("1700000000000".to_owned()),
            compose_time: Some("2026-08-28T09:19:18Z".to_owned()),
            original_arrival_time: None,
            im_display_name: Some("Test User".to_owned()),
            from: Some("8:orgid:11111111-1111-1111-1111-111111111111".to_owned()),
            content: Some("<p>Hello &amp; welcome</p>".to_owned()),
            messagetype: Some("RichText/Html".to_owned()),
            properties: None,
        },
        Vec::new(),
        None,
    )
    .expect("a rich text message should be displayed");

    assert_eq!(message.id, "1700000000000");
    assert_eq!(message.sender, "Test User");
    assert_eq!(
        message.sender_id.as_deref(),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(message.timestamp, "2026-08-28T09:19:18Z");
    assert_eq!(message.content, "Hello & welcome");
    assert!(message.reactions.is_empty());
}

#[test]
fn duplicate_message_ids_are_removed_when_pages_overlap() {
    let messages = order_chat_messages_chronologically(vec![
        ChatMessage {
            id: "same".to_owned(),
            sender: "brandonp2412".to_owned(),
            sender_id: None,
            is_from_current_user: true,
            timestamp: "2026-08-31T00:00:00Z".to_owned(),
            content: "Hello".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
        ChatMessage {
            id: "same".to_owned(),
            sender: "brandonp2412".to_owned(),
            sender_id: None,
            is_from_current_user: true,
            timestamp: "2026-08-31T00:00:00Z".to_owned(),
            content: "Hello".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
    ]);

    assert_eq!(messages.len(), 1);
}

#[test]
fn duplicate_current_user_messages_with_different_ids_are_removed() {
    let messages = order_chat_messages_chronologically(vec![
        ChatMessage {
            id: "client-copy".to_owned(),
            sender: "brandonp2412".to_owned(),
            sender_id: None,
            is_from_current_user: true,
            timestamp: "2026-08-31T00:00:00.123Z".to_owned(),
            content: "Same outgoing message".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
        ChatMessage {
            id: "server-copy".to_owned(),
            sender: "Unknown sender".to_owned(),
            sender_id: Some("different-shape".to_owned()),
            is_from_current_user: true,
            timestamp: "2026-08-31T00:00:00.123Z".to_owned(),
            content: "Same outgoing message".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
    ]);

    assert_eq!(messages.len(), 1);
}

#[test]
fn direct_messages_are_ordered_chronologically_when_the_service_swaps_recent_messages() {
    let messages = order_chat_messages_chronologically(vec![
        ChatMessage {
            id: "newest".to_owned(),
            sender: "Test Peer".to_owned(),
            sender_id: None,
            is_from_current_user: false,
            timestamp: "2026-08-28T09:20:00Z".to_owned(),
            content: "can see inspiration".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
        ChatMessage {
            id: "previous".to_owned(),
            sender: "Test Peer".to_owned(),
            sender_id: None,
            is_from_current_user: false,
            timestamp: "2026-08-28T09:19:00Z".to_owned(),
            content: "tell it to not use Mockito".to_owned(),
            quotes: Vec::new(),
            images: Vec::new(),
            image_urls: Vec::new(),
            reactions: Vec::new(),
        },
    ]);

    assert_eq!(messages[0].id, "previous");
    assert_eq!(messages[1].id, "newest");
}

#[test]
fn native_reaction_counts_parse_string_encoded_emotions() {
    let reactions = native_reaction_counts(
            Some(serde_json::Value::String(
                r#"[{"key":"like","users":[{"mri":"8:orgid:a","time":1},{"mri":"8:orgid:b","time":2}]},{"key":"heart","users":[{"mri":"8:orgid:c","time":3}]},{"key":"follow","users":[{"mri":"8:orgid:d","time":4}]}]"#.to_owned(),
            )),
            None,
        );

    assert_eq!(reactions.len(), 2);
    assert_eq!(reactions[0].reaction_type, "heart");
    assert_eq!(reactions[0].count, 1);
    assert_eq!(reactions[1].reaction_type, "like");
    assert_eq!(reactions[1].count, 2);
}

#[test]
fn native_reaction_counts_parse_array_emotions() {
    let reactions = native_reaction_counts(
        Some(serde_json::json!([
            {"key": "laugh", "users": [{"mri": "8:orgid:a", "time": 1}]},
            {"key": "sad", "users": []}
        ])),
        None,
    );

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].reaction_type, "laugh");
    assert_eq!(reactions[0].count, 1);
}

#[test]
fn reaction_counts_aggregates_graph_reactions() {
    let reactions = reaction_counts(
        Some(vec![
            GraphReaction {
                reaction_type: Some("like".to_owned()),
                user: None,
            },
            GraphReaction {
                reaction_type: Some("like".to_owned()),
                user: None,
            },
            GraphReaction {
                reaction_type: Some("heart".to_owned()),
                user: None,
            },
        ]),
        None,
    );

    assert_eq!(reactions.len(), 2);
    assert_eq!(reactions[0].reaction_type, "heart");
    assert_eq!(reactions[0].count, 1);
    assert_eq!(reactions[1].reaction_type, "like");
    assert_eq!(reactions[1].count, 2);
}

#[test]
fn native_reactions_mark_the_current_users_historical_reaction() {
    let user_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let reactions = native_reaction_counts(
        Some(serde_json::json!([
            {
                "key": "like",
                "users": [
                    {"mri": format!("8:orgid:{user_id}"), "displayName": "Ada", "time": 1},
                    {"mri": "8:orgid:bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "time": 2}
                ]
            }
        ])),
        Some(user_id),
    );

    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].count, 2);
    assert!(reactions[0].selected);
    assert_eq!(reactions[0].users.len(), 2);
    assert_eq!(reactions[0].users[0].id, user_id);
    assert_eq!(reactions[0].users[0].name, "Ada");
}

#[test]
fn graph_reactions_mark_the_current_users_historical_reaction() {
    let user_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let reactions = reaction_counts(
        Some(vec![GraphReaction {
            reaction_type: Some("heart".to_owned()),
            user: Some(GraphReactionIdentitySet {
                user: Some(GraphMessageUser {
                    id: Some(user_id.to_owned()),
                    display_name: Some("Ada".to_owned()),
                }),
            }),
        }]),
        Some(user_id),
    );

    assert_eq!(reactions.len(), 1);
    assert!(reactions[0].selected);
    assert_eq!(reactions[0].users[0].id, user_id);
    assert_eq!(reactions[0].users[0].name, "Ada");
}

#[test]
fn display_name_or_id_uses_id_for_blank_names() {
    assert_eq!(display_name_or_id(Some(""), "channel-1"), "channel-1");
    assert_eq!(display_name_or_id(Some("   "), "channel-2"), "channel-2");
    assert_eq!(display_name_or_id(None, "channel-3"), "channel-3");
}

#[test]
fn display_name_or_id_preserves_named_channels() {
    assert_eq!(display_name_or_id(Some("General"), "channel-1"), "General");
}

#[test]
fn chat_name_or_id_excludes_chats_without_a_display_name() {
    assert_eq!(
        chat_name_or_id(Some(" "), None, Some(""), None, "chat-1"),
        None
    );
}

#[test]
fn chat_name_or_id_excludes_system_conversations() {
    assert_eq!(
        chat_name_or_id(None, None, None, None, "48:notifications"),
        None
    );
}

#[test]
fn is_user_chat_id_excludes_system_conversations() {
    assert!(!super::is_user_chat_id("48:notifications"));
    assert!(!super::is_user_chat_id("48:mentions"));
    assert!(!super::is_user_chat_id("48:threads"));
    assert!(super::is_user_chat_id("48:notes"));
    assert!(super::is_user_chat_id("19:chat@unq.gbl.spaces"));
}

#[test]
fn native_notes_conversation_is_the_current_users_self_chat() {
    assert!(super::is_self_chat_id("48:notes", Some("current-user")));
    assert_eq!(
        profile_photo_user_id(None, "48:notes", Some("current-user")),
        Some("current-user".to_owned())
    );
}

#[test]
fn chat_name_or_id_uses_last_message_display_name_when_topic_is_blank() {
    assert_eq!(
        chat_name_or_id(Some(" "), None, Some("Ada Lovelace"), None, "chat-1"),
        Some("Ada Lovelace".to_owned())
    );
}

#[test]
fn chat_name_or_id_does_not_use_the_current_user_as_a_direct_chat_title() {
    assert_eq!(
        chat_name_or_id(
            Some(" "),
            None,
            Some("brandonp2412"),
            Some("brandonp2412"),
            "chat-1"
        ),
        None
    );
}

#[test]
fn chat_name_or_id_uses_other_members_when_the_current_user_sent_the_last_message() {
    let member_names = ["Test User".to_owned()];
    assert_eq!(
        chat_name_or_id(
            Some(" "),
            Some(&member_names),
            Some("brandonp2412"),
            Some("brandonp2412"),
            "chat-1",
        ),
        Some("Test User".to_owned())
    );
}

#[test]
fn chat_name_or_id_ignores_undefined_topic() {
    assert_eq!(
        chat_name_or_id(
            Some("undefined"),
            None,
            Some("Ada Lovelace"),
            None,
            "chat-1"
        ),
        Some("Ada Lovelace".to_owned())
    );
}

#[test]
fn chat_name_or_id_prefers_a_non_blank_topic() {
    assert_eq!(
        chat_name_or_id(
            Some("Project discussion"),
            None,
            Some("Ada Lovelace"),
            None,
            "chat-1"
        ),
        Some("Project discussion".to_owned())
    );
}

#[test]
fn profile_photo_user_id_uses_the_other_direct_chat_participant() {
    assert_eq!(
        profile_photo_user_id(
            None,
            "19:11111111-1111-1111-1111-111111111111_22222222-2222-2222-2222-222222222222@unq.gbl.spaces",
            Some("11111111-1111-1111-1111-111111111111"),
        ),
        Some("22222222-2222-2222-2222-222222222222".to_owned()),
    );
}

#[test]
fn identifies_a_graph_self_chat_from_its_members() {
    let members = [
        GraphChatMember {
            display_name: None,
            user_id: Some("current-user".to_owned()),
        },
        GraphChatMember {
            display_name: None,
            user_id: Some("current-user".to_owned()),
        },
    ];

    assert!(is_graph_self_chat(
        Some("oneOnOne"),
        &members,
        Some("current-user"),
    ));
}

#[test]
fn cached_chat_lists_are_refreshed_when_online() {
    assert!(should_refresh_chats());
}

#[test]
fn native_sender_id_ignores_non_organisation_identities() {
    assert_eq!(native_sender_id(Some("8:acs:guest")), None);
}

#[test]
fn internal_group_ids_are_not_display_names() {
    assert!(!is_displayable_chat_name(
        "19:FwN1x7eGmAnInternalGroupHash@thread.v2"
    ));
    assert!(is_displayable_chat_name("Release planning"));
}

#[test]
fn remote_image_urls_are_extracted_from_teams_markup() {
    assert_eq!(
        image_src_urls(
            r#"<p><img src="https://au-prod.asyncgw.teams.microsoft.com/v1/objects/abc/views/imgo"><img data-orig-src="https://eu-api.asm.skype.com/v1/objects/def/views/imgo"></p>"#,
        ),
        vec![
            "https://au-prod.asyncgw.teams.microsoft.com/v1/objects/abc/views/imgo",
            "https://eu-api.asm.skype.com/v1/objects/def/views/imgo",
        ]
    );
}

#[test]
fn gif_urls_are_extracted_from_teams_card_content() {
    assert_eq!(
        image_src_urls(
            r#"{"images":[{"url":"https://media.giphy.com/media/abc123/giphy.gif?cid=teams"}]}"#,
        ),
        vec!["https://media.giphy.com/media/abc123/giphy.gif?cid=teams"]
    );
}

#[test]
fn hosted_image_ids_are_extracted_once() {
    assert_eq!(
        hosted_content_ids(
            r#"<p>photo</p><img src="../hostedContents/1/$value"><img src="../hostedContents/1/$value"><img src="../hostedContents/2/$value">"#,
        ),
        vec!["1".to_owned(), "2".to_owned()]
    );
}

#[test]
fn user_ids_match_case_insensitively_and_ignore_braces() {
    assert!(ids_equal(
        "{ABCDEF12-3456-7890-ABCD-EF1234567890}",
        "abcdef12-3456-7890-abcd-ef1234567890"
    ));
}
