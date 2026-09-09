part of '../workspace.dart';

extension _WorkspaceCallActions on _TeamsWorkspaceState {
  void _setRingback(bool enabled) {
    if (_ringbackActive == enabled) return;
    _ringbackActive = enabled;
    unawaited(
      (enabled
              ? PlatformCallAudio.startRingback()
              : PlatformCallAudio.stopRingback())
          .catchError((Object error) => AppLog.record('Call ringback', error)),
    );
  }

  void _onCallUpdate(CallUpdate update) {
    AppLog.info(
      'Teams call',
      '${update.kind.name} callId=${update.callId} '
          'conversationId=${update.conversationId ?? '<none>'} '
          '${update.detail ?? ''}',
    );
    if (!mounted) return;

    final current = _callUpdate;
    if (current == null) {
      if (update.kind != CallUpdateKind.incoming) {
        AppLog.info(
          'Teams call',
          'Ignored stale ${update.kind.name} update for ${update.callId}',
        );
        return;
      }
    } else {
      final adoptsBackendCallId =
          current.callId.startsWith('local:') &&
          update.kind != CallUpdateKind.incoming &&
          current.conversationId != null &&
          current.conversationId == update.conversationId;
      final sameCall = current.callId == update.callId || adoptsBackendCallId;
      if (!sameCall) {
        AppLog.info(
          'Teams call',
          'Ignored ${update.kind.name} update for ${update.callId} while ${current.callId} is active',
        );
        return;
      }
    }
    if (update.kind == CallUpdateKind.incoming ||
        update.kind == CallUpdateKind.dialing ||
        update.kind == CallUpdateKind.ringing) {
      _setRingback(true);
    } else {
      _setRingback(false);
    }
    if (update.kind == CallUpdateKind.error) {
      AppLog.record(
        'Teams call ${update.conversationId ?? update.callId}',
        update.detail ?? 'The Teams call failed.',
      );
      unawaited(PlatformCallAudio.reset());
      if (_videoCallActive) unawaited(PlatformCallVideo.stopCamera());
      _mutate(() {
        _videoCallActive = false;
        _callUpdate = null;
        _error = update.detail ?? 'The Teams call failed.';
      });
      return;
    }
    if (update.kind == CallUpdateKind.ended) {
      if (current != null && current.kind != CallUpdateKind.connected) {
        AppLog.record(
          'Teams call ended before connection',
          update.detail ??
              'Backend ended ${update.callId} while ${current.kind.name}',
        );
      }
      unawaited(PlatformCallAudio.reset());
      if (_videoCallActive) unawaited(PlatformCallVideo.stopCamera());
      _mutate(() {
        _videoCallActive = false;
        _callUpdate = null;
      });
      return;
    }
    final mergedUpdate = current == null
        ? update
        : CallUpdate(
            kind: update.kind,
            callId: update.callId,
            conversationId: update.conversationId ?? current.conversationId,
            displayName: update.displayName ?? current.displayName,
            detail: update.detail ?? current.detail,
          );
    _mutate(() {
      if (mergedUpdate.kind == CallUpdateKind.incoming) {
        _videoCallActive = mergedUpdate.detail?.contains('video=true') == true;
      } else if (mergedUpdate.detail?.contains('video=true') == true) {
        _videoCallActive = true;
      }
      _callUpdate = mergedUpdate;
    });
  }

  Future<bool> _ensureMicrophonePermission() async {
    if (kIsWeb ||
        (!Platform.isAndroid && !Platform.isIOS && !Platform.isMacOS)) {
      return true;
    }
    final status = await Permission.microphone.request();
    if (status.isGranted) return true;
    if (mounted) {
      _mutate(
        () => _error = status.isPermanentlyDenied
            ? 'Microphone permission is disabled. Enable it in system settings to use calls.'
            : 'Microphone permission is required to make or receive calls.',
      );
    }
    return false;
  }

  Future<bool> _ensureCameraPermission() async {
    if (kIsWeb || !Platform.isAndroid) return true;
    final status = await Permission.camera.request();
    if (status.isGranted) return true;
    if (mounted) {
      _mutate(
        () => _error = status.isPermanentlyDenied
            ? 'Camera permission is disabled. Enable it in system settings to use video calls.'
            : 'Camera permission is required to make video calls.',
      );
    }
    return false;
  }

  Future<void> _runTestCall() async {
    if (_callActionBusy || _callUpdate != null) {
      throw StateError('A call is already active');
    }
    _mutate(() => _callActionBusy = true);
    unawaited(PlatformCallAudio.startRingback());
    try {
      if (!await _ensureMicrophonePermission()) {
        throw StateError('Microphone permission is required for a test call');
      }
      await _ensureCameraPermission();
      if (!mounted) return;
      await Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => _TestCallScreen(callGateway: widget.callGateway),
        ),
      );
    } catch (_) {
      await PlatformCallAudio.reset();
      rethrow;
    } finally {
      if (mounted) _mutate(() => _callActionBusy = false);
    }
  }

  Future<void> _startCall(
    Conversation conversation, {
    bool video = false,
  }) async {
    if (_callActionBusy || _callUpdate != null) return;
    final callGateway = widget.callGateway;
    var cameraStarted = false;
    _mutate(() => _callActionBusy = true);
    try {
      if (!await _ensureMicrophonePermission() || !mounted) return;
      if (video) {
        if (kIsWeb) {
          throw StateError('Video calls are not supported in the browser yet.');
        }
        if (!await _ensureCameraPermission() || !mounted) return;
        if (PlatformCallVideo.supported) {
          cameraStarted = await PlatformCallVideo.startCamera();
          if (!mounted) {
            if (cameraStarted) await PlatformCallVideo.stopCamera();
            return;
          }
          if (!cameraStarted) {
            throw StateError(
              'No usable camera is available for this video call.',
            );
          }
        }
      }
      String? calleeUserId;
      for (final message in _messages.reversed) {
        final senderId = message.senderId?.trim();
        if (!message.isFromCurrentUser && senderId?.isNotEmpty == true) {
          calleeUserId = senderId;
          break;
        }
      }
      calleeUserId ??= conversation.profilePhotoUserId;
      if (calleeUserId == null && !conversation.isGroup) {
        final currentUserId = _user?.id;
        for (final memberId in conversation.avatarUserIds) {
          if (memberId.trim().isNotEmpty &&
              !_sameUserId(memberId, currentUserId)) {
            calleeUserId = memberId.trim();
            break;
          }
        }
      }
      _mutate(() {
        _videoCallActive = video;
        _callUpdate = CallUpdate(
          kind: CallUpdateKind.dialing,
          callId: 'local:${DateTime.now().microsecondsSinceEpoch}',
          conversationId: conversation.id,
          displayName: conversation.name,
        );
      });
      _setRingback(true);
      unawaited(PlatformCallAudio.setSpeakerphone(true));
      await callGateway.startCall(
        conversation.id,
        video: video,
        calleeUserId: calleeUserId,
      );
      if (!mounted) {
        try {
          await callGateway.hangUp();
        } catch (error, stackTrace) {
          AppLog.record('Hang up late-started Teams call', error, stackTrace);
        }
        if (cameraStarted) {
          try {
            await PlatformCallVideo.stopCamera();
          } catch (error, stackTrace) {
            AppLog.record('Stop late-started call camera', error, stackTrace);
          }
        }
        try {
          await PlatformCallAudio.reset();
        } catch (error, stackTrace) {
          AppLog.record('Reset late-started call audio', error, stackTrace);
        }
      }
    } catch (error) {
      if (cameraStarted) {
        unawaited(PlatformCallVideo.stopCamera());
      }
      AppLog.record('Start call to ${conversation.name}', error);
      if (mounted) {
        _mutate(() {
          _videoCallActive = false;
          _callUpdate = null;
          _error = error;
        });
      }
    } finally {
      if (mounted) _mutate(() => _callActionBusy = false);
    }
  }

  Future<void> _hangUp() async {
    if (_callActionBusy) return;
    final call = _callUpdate;
    final videoActive = _videoCallActive;
    _mutate(() => _callActionBusy = true);
    _setRingback(false);
    try {
      if (mounted) _mutate(() => _callUpdate = null);
      await widget.callGateway.hangUp();
      if (_videoCallActive) {
        await PlatformCallVideo.stopCamera();
        _videoCallActive = false;
      }
      await PlatformCallAudio.reset();
    } catch (error) {
      AppLog.record('Hang up Teams call', error);
      if (mounted) {
        _mutate(() {
          _callUpdate = call;
          _videoCallActive = videoActive;
          _error = error;
        });
      }
    } finally {
      if (mounted) _mutate(() => _callActionBusy = false);
    }
  }

  Future<bool> _endCallForAccountTransition() async {
    final call = _callUpdate;
    if (call == null) return true;
    if (_callActionBusy) return false;
    final videoActive = _videoCallActive;
    _mutate(() => _callActionBusy = true);
    _setRingback(false);
    try {
      if (call.kind == CallUpdateKind.incoming) {
        await widget.callGateway.declineCall(call.callId);
      } else {
        await widget.callGateway.hangUp();
      }
    } catch (error, stackTrace) {
      AppLog.record('End Teams call before account change', error, stackTrace);
      if (mounted) {
        _mutate(() {
          _callActionBusy = false;
          _error = error;
        });
        if (call.kind == CallUpdateKind.incoming ||
            call.kind == CallUpdateKind.dialing ||
            call.kind == CallUpdateKind.ringing) {
          _setRingback(true);
        }
      }
      return false;
    }

    if (mounted) {
      _mutate(() {
        _callActionBusy = false;
        _callUpdate = null;
        _videoCallActive = false;
      });
    }
    try {
      if (videoActive) await PlatformCallVideo.stopCamera();
      await PlatformCallAudio.reset();
    } catch (error, stackTrace) {
      AppLog.record(
        'Reset call media before account change',
        error,
        stackTrace,
      );
    }
    return true;
  }

  Future<void> _acceptIncomingCall() async {
    final call = _callUpdate;
    if (call == null ||
        call.kind != CallUpdateKind.incoming ||
        _callActionBusy) {
      return;
    }
    _mutate(() => _callActionBusy = true);
    try {
      if (!await _ensureMicrophonePermission()) return;
      await PlatformCallAudio.setSpeakerphone(true);
      await widget.callGateway.acceptCall(call.callId);
    } catch (error) {
      AppLog.record('Accept Teams call', error);
      if (mounted) {
        _mutate(() {
          _callUpdate = call;
          _error = error;
        });
      }
    } finally {
      if (mounted) _mutate(() => _callActionBusy = false);
    }
  }

  Future<void> _declineIncomingCall() async {
    final call = _callUpdate;
    if (call == null ||
        call.kind != CallUpdateKind.incoming ||
        _callActionBusy) {
      return;
    }
    _mutate(() {
      _callActionBusy = true;
      _callUpdate = null;
    });
    try {
      _setRingback(false);
      await widget.callGateway.declineCall(call.callId);
    } catch (error) {
      AppLog.record('Decline Teams call', error);
      if (mounted) {
        _mutate(() {
          _callUpdate = call;
          _error = error;
        });
      }
    } finally {
      if (mounted) _mutate(() => _callActionBusy = false);
    }
  }

  String _callDisplayName(CallUpdate call) {
    if (call.displayName?.trim().isNotEmpty == true) return call.displayName!;
    final id = call.conversationId;
    if (id != null) {
      for (final chat in _chats) {
        if (chat.id == id) return chat.name;
      }
    }
    return 'Teams call';
  }
}
