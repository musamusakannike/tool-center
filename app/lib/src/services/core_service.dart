import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:url_launcher/url_launcher.dart';

import '../models/tool_models.dart';
import '../rust/api.dart' as rust_api;
import '../rust/frb_generated.dart';

abstract class ToolCenterBackend {
  Future<void> initialize();
  String getCoreVersion();
  List<ToolDescriptorModel> listTools();
  List<JobSnapshotModel> listJobs();
  Stream<JobSnapshotModel> subscribeJobSnapshots();
  Future<String> submitJob({
    required String toolId,
    required List<String> inputs,
    required Map<String, dynamic> options,
    String? outputDir,
  });
  bool cancelJob(String jobId);
  void clearFinishedJobs();
  Future<(List<AiModelStatusModel>, List<FlaggedExcludedModelInfo>)>
  listAiModels();
  Future<bool> deleteAiModel(String modelId);
}

class RustBridgeBackend implements ToolCenterBackend {
  static bool _initialized = false;

  @override
  Future<void> initialize() async {
    if (_initialized) return;
    final externalLib = _tryResolveCustomLibrary();
    await RustLib.init(externalLibrary: externalLib);
    _initialized = true;
  }

  ExternalLibrary? _tryResolveCustomLibrary() {
    final exeDir = File(Platform.resolvedExecutable).parent.path;
    final cwd = Directory.current.path;
    final libFileName = Platform.isWindows
        ? 'bridge.dll'
        : Platform.isMacOS
        ? 'libbridge.dylib'
        : 'libbridge.so';

    final candidates = <String>[
      p.join(exeDir, 'lib', libFileName),
      p.join(exeDir, libFileName),
      p.join(exeDir, '..', 'Frameworks', libFileName),
      p.join(cwd, '..', 'core', 'target', 'release', libFileName),
      p.join(cwd, '..', 'core', 'target', 'debug', libFileName),
      p.join(cwd, 'core', 'target', 'release', libFileName),
      p.join(cwd, 'core', 'target', 'debug', libFileName),
    ];

    for (final candidate in candidates) {
      if (File(candidate).existsSync()) {
        return ExternalLibrary.open(candidate);
      }
    }
    return null;
  }

  @override
  String getCoreVersion() => rust_api.coreVersion();

  @override
  List<ToolDescriptorModel> listTools() {
    return ToolDescriptorModel.decodeList(rust_api.listToolsJson());
  }

  @override
  List<JobSnapshotModel> listJobs() {
    return JobSnapshotModel.decodeList(rust_api.listJobsJson());
  }

  @override
  Stream<JobSnapshotModel> subscribeJobSnapshots() {
    return rust_api.subscribeJobEvents().map((raw) {
      final decoded = jsonDecode(raw) as Map<String, dynamic>;
      return JobSnapshotModel.fromJson(decoded);
    });
  }

  @override
  Future<String> submitJob({
    required String toolId,
    required List<String> inputs,
    required Map<String, dynamic> options,
    String? outputDir,
  }) {
    return rust_api.submitToolJob(
      toolId: toolId,
      inputs: inputs,
      optionsJson: jsonEncode(options),
      outputDir: outputDir,
    );
  }

  @override
  bool cancelJob(String jobId) => rust_api.cancelJob(jobId: jobId);

  @override
  void clearFinishedJobs() => rust_api.clearFinishedJobs();

  Future<String> _resolveAiCacheDir() async {
    try {
      final supportDir = await getApplicationSupportDirectory();
      return p.join(supportDir.path, 'ai_models');
    } catch (_) {
      return p.join(Directory.systemTemp.path, 'tool_center_ai_models');
    }
  }

  @override
  Future<(List<AiModelStatusModel>, List<FlaggedExcludedModelInfo>)>
  listAiModels() async {
    final dir = await _resolveAiCacheDir();
    final raw = await rust_api.listAiModelsJson(cacheDir: dir);
    final map = jsonDecode(raw) as Map<String, dynamic>;
    final models = (map['models'] as List<dynamic>? ?? [])
        .whereType<Map<String, dynamic>>()
        .map(AiModelStatusModel.fromJson)
        .toList();
    final flagged = (map['flagged_excluded'] as List<dynamic>? ?? [])
        .whereType<Map<String, dynamic>>()
        .map(FlaggedExcludedModelInfo.fromJson)
        .toList();
    return (models, flagged);
  }

  @override
  Future<bool> deleteAiModel(String modelId) async {
    final dir = await _resolveAiCacheDir();
    return rust_api.deleteAiModel(cacheDir: dir, modelId: modelId);
  }
}

/// Lightweight in-memory backend for isolated Flutter widget tests.
class FakeToolCenterBackend implements ToolCenterBackend {
  final List<ToolDescriptorModel> tools;
  final List<JobSnapshotModel> jobs;
  final StreamController<JobSnapshotModel> _events =
      StreamController<JobSnapshotModel>.broadcast();

  FakeToolCenterBackend({this.tools = const [], List<JobSnapshotModel>? jobs})
    : jobs = List<JobSnapshotModel>.from(jobs ?? const []);

  @override
  Future<void> initialize() async {}

  @override
  String getCoreVersion() => '0.1.0-test';

  @override
  List<ToolDescriptorModel> listTools() => tools;

  @override
  List<JobSnapshotModel> listJobs() => List.unmodifiable(jobs);

  @override
  Stream<JobSnapshotModel> subscribeJobSnapshots() => _events.stream;

  @override
  Future<String> submitJob({
    required String toolId,
    required List<String> inputs,
    required Map<String, dynamic> options,
    String? outputDir,
  }) async {
    final id = 'job-${jobs.length + 1}';
    final snap = JobSnapshotModel(
      jobId: id,
      toolId: toolId,
      toolName: toolId,
      status: JobStatusId.completed,
      progress: 1.0,
      stepMessage: 'Completed',
      inputCount: inputs.length,
      outputs: const [],
      summary: 'Done',
      createdAtMs: DateTime.now().millisecondsSinceEpoch,
      durationMs: 12,
    );
    jobs.insert(0, snap);
    _events.add(snap);
    return id;
  }

  @override
  bool cancelJob(String jobId) => true;

  @override
  void clearFinishedJobs() {
    jobs.removeWhere((j) => !j.status.isActive);
  }

  @override
  Future<(List<AiModelStatusModel>, List<FlaggedExcludedModelInfo>)>
  listAiModels() async {
    return (const <AiModelStatusModel>[], const <FlaggedExcludedModelInfo>[]);
  }

  @override
  Future<bool> deleteAiModel(String modelId) async => false;
}

/// Reveals a file or folder in the user's native OS file manager without blocking.
Future<void> revealInSystemFolder(String targetPath) async {
  final entity = FileSystemEntity.isDirectorySync(targetPath)
      ? Directory(targetPath)
      : File(targetPath);
  final parentDir = entity is Directory ? entity.path : entity.parent.path;

  if (Platform.isWindows) {
    if (entity is File && entity.existsSync()) {
      await Process.start('explorer.exe', ['/select,', entity.path]);
    } else {
      await Process.start('explorer.exe', [parentDir]);
    }
  } else if (Platform.isMacOS) {
    if (entity.existsSync()) {
      await Process.start('open', ['-R', entity.path]);
    } else {
      await Process.start('open', [parentDir]);
    }
  } else {
    await Process.start('xdg-open', [parentDir]);
  }
}

/// Opens a file with the user's default system application (e.g. DOCX/XLSX/PPTX fallback).
Future<void> openInSystemApp(String filePath) async {
  final uri = Uri.file(filePath);
  if (!await launchUrl(uri)) {
    if (Platform.isWindows) {
      await Process.start('cmd', ['/c', 'start', '', filePath]);
    } else if (Platform.isMacOS) {
      await Process.start('open', [filePath]);
    } else {
      await Process.start('xdg-open', [filePath]);
    }
  }
}
