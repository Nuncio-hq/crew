import 'dart:io';

/// Removes a voice-note test's owned temporary directory.
Future<void> deleteVoiceNoteFixtureDirectory(Directory directory) async {
  if (!await directory.exists()) return;
  for (var attempt = 0; attempt < 2; attempt++) {
    try {
      await directory.delete(recursive: true);
      break;
    } on PathNotFoundException {
      // Player disposal can remove a child during recursive traversal.
      if (!await directory.exists()) return;
      if (attempt == 1) rethrow;
    }
  }
  if (await directory.exists()) {
    throw FileSystemException(
      'Fixture directory remains after cleanup',
      directory.path,
    );
  }
}
