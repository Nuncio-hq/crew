import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'voice_note_fixture_cleanup.dart';

/// Controls only filesystem scheduling/errors; the helper under test is real.
class _ControlledDirectory implements Directory {
  _ControlledDirectory(this.directory, {this.remove});

  final Directory directory;
  final Future<FileSystemEntity> Function(int attempt)? remove;
  int deleteCalls = 0;

  @override
  String get path => directory.path;

  @override
  Future<bool> exists() => directory.exists();

  @override
  Future<FileSystemEntity> delete({bool recursive = false}) {
    expect(recursive, isTrue);
    deleteCalls++;
    return remove?.call(deleteCalls) ?? directory.delete(recursive: recursive);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

Future<Directory> fixture() async {
  final directory = await Directory.systemTemp.createTemp('voice-cleanup-test');
  addTearDown(() {
    if (directory.existsSync()) directory.deleteSync(recursive: true);
  });
  await File('${directory.path}/recording.m4a').writeAsString('fixture');
  return directory;
}

void main() {
  test('removes an existing owned directory and its contents', () async {
    final directory = await fixture();
    final controlled = _ControlledDirectory(directory);
    await deleteVoiceNoteFixtureDirectory(controlled);
    expect(controlled.deleteCalls, 1);
    expect(await directory.exists(), isFalse);
  });

  test('already absent fixture requires no delete attempt', () async {
    final directory = await fixture();
    await directory.delete(recursive: true);
    final controlled = _ControlledDirectory(directory);
    await deleteVoiceNoteFixtureDirectory(controlled);
    expect(controlled.deleteCalls, 0);
    expect(await directory.exists(), isFalse);
  });

  test(
    'root disappearing after the existence check is successful cleanup',
    () async {
      final directory = await fixture();
      final started = Completer<void>();
      final resume = Completer<void>();
      final controlled = _ControlledDirectory(
        directory,
        remove: (_) async {
          started.complete();
          await resume.future;
          return directory.delete(recursive: true);
        },
      );
      final cleanup = expectLater(
        deleteVoiceNoteFixtureDirectory(controlled),
        completes,
      );
      await started.future;
      await directory.delete(recursive: true);
      resume.complete();
      await cleanup;
      expect(controlled.deleteCalls, 1);
      expect(await directory.exists(), isFalse);
    },
  );

  test(
    'child disappearance retries once and removes the remaining root',
    () async {
      final directory = await fixture();
      final controlled = _ControlledDirectory(
        directory,
        remove: (attempt) async {
          if (attempt == 1) {
            await File('${directory.path}/recording.m4a').delete();
            // Models recursive traversal reporting the child removed by dispose.
            throw PathNotFoundException(
              directory.path,
              const OSError('gone', 2),
            );
          }
          return directory.delete(recursive: true);
        },
      );
      await deleteVoiceNoteFixtureDirectory(controlled);
      expect(controlled.deleteCalls, 2);
      expect(await directory.exists(), isFalse);
    },
  );

  test(
    'persistent missing-path error with a remaining root propagates',
    () async {
      final directory = await fixture();
      final errors = [
        PathNotFoundException(directory.path, const OSError('first', 2)),
        PathNotFoundException(directory.path, const OSError('second', 2)),
      ];
      final controlled = _ControlledDirectory(
        directory,
        remove: (attempt) async {
          throw errors[(attempt - 1).clamp(0, 1)];
        },
      );
      await expectLater(
        deleteVoiceNoteFixtureDirectory(controlled),
        throwsA(same(errors.last)),
      );
      expect(controlled.deleteCalls, 2);
      expect(await directory.exists(), isTrue);
    },
  );

  test('permission errors propagate without a retry', () async {
    final directory = await fixture();
    final error = FileSystemException(
      'denied',
      directory.path,
      const OSError('denied', 13),
    );
    final controlled = _ControlledDirectory(
      directory,
      remove: (_) async => throw error,
    );
    await expectLater(
      deleteVoiceNoteFixtureDirectory(controlled),
      throwsA(same(error)),
    );
    expect(controlled.deleteCalls, 1);
    expect(await directory.exists(), isTrue);
  });

  test('a different error on the retry still propagates', () async {
    final directory = await fixture();
    final error = FileSystemException(
      'I/O error',
      directory.path,
      const OSError('I/O error', 5),
    );
    final controlled = _ControlledDirectory(
      directory,
      remove: (attempt) async {
        if (attempt == 1) {
          throw PathNotFoundException(directory.path, const OSError('gone', 2));
        }
        throw error;
      },
    );
    await expectLater(
      deleteVoiceNoteFixtureDirectory(controlled),
      throwsA(same(error)),
    );
    expect(controlled.deleteCalls, 2);
    expect(await directory.exists(), isTrue);
  });

  test('delete returning without removing the root is not success', () async {
    final directory = await fixture();
    final controlled = _ControlledDirectory(
      directory,
      remove: (_) async => directory,
    );
    await expectLater(
      deleteVoiceNoteFixtureDirectory(controlled),
      throwsA(isA<FileSystemException>()),
    );
    expect(controlled.deleteCalls, 1);
    expect(await directory.exists(), isTrue);
  });
}
