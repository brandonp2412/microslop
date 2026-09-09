import 'teams_gateway.dart';

class WorkspaceDatabase {
  Future<void> initialize() async {}

  Future<void> close() async {}

  Future<void> clear() async {}

  Future<List<MessageSummary>> loadMessages(Conversation conversation) async =>
      const [];

  Future<List<MessageImage>> loadImages(
    Conversation conversation,
    String messageId,
  ) async => const [];

  Future<void> saveMessages(
    Conversation conversation,
    List<MessageSummary> messages, {
    bool replaceImages = true,
  }) async {}

  Future<void> saveImages(
    Conversation conversation,
    List<MessageSummary> messages,
  ) async {}
}
