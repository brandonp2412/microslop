import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sqlite3/sqlite3.dart';

void main() {
  test('workspace sqlite dependency is SQLCipher and encrypts files', () {
    final directory = Directory.systemTemp.createTempSync(
      'microslop-sqlcipher-',
    );
    addTearDown(() => directory.deleteSync(recursive: true));
    final path = '${directory.path}/cache.db';
    const key =
        '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff';

    final database = sqlite3.open(path);
    database.execute("PRAGMA key = \"x'$key'\"");
    final cipherVersion = database.select('PRAGMA cipher_version');
    expect(cipherVersion, isNotEmpty);
    expect(cipherVersion.first.values.first.toString(), isNotEmpty);
    database.execute('CREATE TABLE sample(value TEXT NOT NULL)');
    database.execute('INSERT INTO sample(value) VALUES(?)', ['secret']);
    database.close();

    final header = String.fromCharCodes(File(path).readAsBytesSync().take(16));
    expect(header, isNot('SQLite format 3\u0000'));

    final reopened = sqlite3.open(path);
    reopened.execute("PRAGMA key = \"x'$key'\"");
    expect(
      reopened.select('SELECT value FROM sample').single['value'],
      'secret',
    );
    reopened.close();
  });
}
