import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/workspace_cache.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  test('bench selected conversation persistence', () async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    final snapshot = CachedWorkspace(
      chats: [
        for (var i = 0; i < 500; i++)
          CachedConversation(
            id: 'chat-$i',
            name: 'Conversation $i',
            isGroup: i.isEven,
            memberUserIds: ['user-$i', 'user-${i + 1}'],
            lastMessageId: 'message-$i',
            preview: 'Preview text for conversation $i',
          ),
      ],
      teams: [
        for (var team = 0; team < 20; team++)
          CachedTeam(
            id: 'team-$team',
            name: 'Team $team',
            channels: [
              for (var channel = 0; channel < 20; channel++)
                CachedConversation(
                  id: 'channel-$team-$channel',
                  name: 'Channel $channel',
                  isGroup: false,
                  teamId: 'team-$team',
                ),
            ],
          ),
      ],
      selectedChatId: 'chat-250',
      messages: const {},
    );
    final samplesFull = <int>[];
    final samplesSelected = <int>[];
    for (var i = 0; i < 25; i++) {
      var watch = Stopwatch()..start();
      await prefs.setString('workspace.chats', jsonEncode(snapshot.toJson()));
      watch.stop();
      samplesFull.add(watch.elapsedMicroseconds);

      watch = Stopwatch()..start();
      await prefs.setString('workspace.selectedChatId', 'chat-${i % 500}');
      watch.stop();
      samplesSelected.add(watch.elapsedMicroseconds);
    }
    samplesFull.sort();
    samplesSelected.sort();
    final fullP50 = samplesFull[samplesFull.length ~/ 2];
    final selectedP50 = samplesSelected[samplesSelected.length ~/ 2];
    final fullP95 = samplesFull[(samplesFull.length * .95).ceil() - 1];
    final selectedP95 =
        samplesSelected[(samplesSelected.length * .95).ceil() - 1];
    print(
      'full_p50_us=$fullP50 selected_p50_us=$selectedP50 gain=${((fullP50 - selectedP50) * 100 / fullP50).toStringAsFixed(1)}%',
    );
    print(
      'full_p95_us=$fullP95 selected_p95_us=$selectedP95 gain=${((fullP95 - selectedP95) * 100 / fullP95).toStringAsFixed(1)}%',
    );
  });
}
