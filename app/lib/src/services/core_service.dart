import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';
import 'package:url_launcher/url_launcher.dart';

import '../models/preview_models.dart';
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
  Future<FileInspectionModel> inspectFile(String path);
  Future<TextPreviewModel> readTextPreview(String path, {int maxBytes = 262144});
  Future<Uint8List> renderDecodedImagePreview(
    String path, {
    int maxDimension = 2048,
  });
  Future<Uint8List> renderPdfPagePreview(
    String path, {
    required int pageIndex,
    int targetWidth = 1100,
    String? password,
  });
  Future<int> getPdfPageCount(String path, {String? password});

  Future<AiModelsCatalogModel> getAiCatalog();
  Stream<AiModelsCatalogModel> subscribeAiCatalog();
  Future<void> startAiModelDownload(String modelId);
  bool cancelAiModelDownload(String modelId);
  Future<String> setAiModelsDir(String? customDir);
  void setAiDevicePreference(String preference);
  Future<String> importAiModel({
    required String modelId,
    required String sourcePath,
  });
  Future<bool> deleteAiModel(String modelId);
}

class RustBridgeBackend implements ToolCenterBackend {
  static bool _initialized = false;
  String? _cachedDefaultAiDir;

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

  @override
  Future<FileInspectionModel> inspectFile(String path) async {
    final raw = await rust_api.inspectFileJson(path: path);
    return FileInspectionModel.fromJsonStr(raw);
  }

  @override
  Future<TextPreviewModel> readTextPreview(
    String path, {
    int maxBytes = 262144,
  }) async {
    final raw = await rust_api.readTextPreviewJson(
      path: path,
      maxBytes: maxBytes,
    );
    return TextPreviewModel.fromJsonStr(raw);
  }

  @override
  Future<Uint8List> renderDecodedImagePreview(
    String path, {
    int maxDimension = 2048,
  }) {
    return rust_api.renderDecodedImagePreview(
      path: path,
      maxDimension: maxDimension,
    );
  }

  @override
  Future<Uint8List> renderPdfPagePreview(
    String path, {
    required int pageIndex,
    int targetWidth = 1100,
    String? password,
  }) {
    return rust_api.renderPdfPagePreviewJson(
      path: path,
      pageIndex: pageIndex,
      targetWidth: targetWidth,
      password: password,
    );
  }

  @override
  Future<int> getPdfPageCount(String path, {String? password}) {
    return rust_api.getPdfPageCount(path: path, password: password);
  }

  Future<String> _resolveAiCacheDir() async {
    if (_cachedDefaultAiDir != null) return _cachedDefaultAiDir!;
    try {
      final supportDir = await getApplicationSupportDirectory().timeout(
        const Duration(milliseconds: 300),
      );
      _cachedDefaultAiDir = p.join(supportDir.path, 'ai_models');
      return _cachedDefaultAiDir!;
    } catch (_) {
      _cachedDefaultAiDir = p.join(
        Directory.systemTemp.path,
        'tool_center_ai_models',
      );
      return _cachedDefaultAiDir!;
    }
  }

  @override
  Future<AiModelsCatalogModel> getAiCatalog() async {
    final dir = await _resolveAiCacheDir();
    final raw = await rust_api.listAiModelsJson(cacheDir: dir);
    return AiModelsCatalogModel.fromJsonStr(raw);
  }

  @override
  Stream<AiModelsCatalogModel> subscribeAiCatalog() {
    return rust_api.subscribeAiModelEvents().map(
      AiModelsCatalogModel.fromJsonStr,
    );
  }

  @override
  Future<void> startAiModelDownload(String modelId) async {
    final dir = await _resolveAiCacheDir();
    await rust_api.startAiModelDownload(cacheDir: dir, modelId: modelId);
  }

  @override
  bool cancelAiModelDownload(String modelId) {
    return rust_api.cancelAiModelDownload(modelId: modelId);
  }

  @override
  Future<String> setAiModelsDir(String? customDir) async {
    final dir = await _resolveAiCacheDir();
    return rust_api.setAiModelsDir(cacheDir: dir, customDir: customDir);
  }

  @override
  void setAiDevicePreference(String preference) {
    rust_api.setAiDevicePreference(preference: preference);
  }

  @override
  Future<String> importAiModel({
    required String modelId,
    required String sourcePath,
  }) async {
    final dir = await _resolveAiCacheDir();
    return rust_api.importAiModelPath(
      cacheDir: dir,
      modelId: modelId,
      sourcePath: sourcePath,
    );
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
  final Map<String, FileInspectionModel> mockInspections;
  final Map<String, TextPreviewModel> mockTextPreviews;
  List<AiModelStatusModel> mockAiModels;
  String _modelsDir = '/tmp/tool_center_ai_models';
  bool _isCustomDir = false;
  String _devicePref = 'auto';

  final StreamController<JobSnapshotModel> _events =
      StreamController<JobSnapshotModel>.broadcast();
  final StreamController<AiModelsCatalogModel> _aiEvents =
      StreamController<AiModelsCatalogModel>.broadcast();

  FakeToolCenterBackend({
    this.tools = const [],
    List<JobSnapshotModel>? jobs,
    this.mockInspections = const {},
    this.mockTextPreviews = const {},
    List<AiModelStatusModel>? mockAiModels,
  }) : jobs = List<JobSnapshotModel>.from(jobs ?? const []),
       mockAiModels = List<AiModelStatusModel>.from(
         mockAiModels ??
             const [
               AiModelStatusModel(
                 id: 'bg.u2netp',
                 displayName: 'U²-Netp (Fast General & Portrait)',
                 task: 'background_removal',
                 defaultForTask: true,
                 spdxLicense: 'Apache-2.0',
                 sourceUrl: 'https://github.com/xuebinqin/U-2-Net',
                 attribution: 'Xuebin Qin et al. (Apache-2.0)',
                 description: 'Fast salient-object background removal.',
                 sizeBytes: 4574861,
                 sha256:
                     '309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8',
                 state: ModelInstallStateId.notInstalled,
                 installed: false,
               ),
               AiModelStatusModel(
                 id: 'ocr.ppocr_v4_en',
                 displayName: 'PP-OCRv4 English & Latin Pack',
                 task: 'ocr',
                 defaultForTask: true,
                 language: 'en',
                 spdxLicense: 'Apache-2.0',
                 sourceUrl: 'https://github.com/PaddlePaddle/PaddleOCR',
                 attribution: 'PaddleOCR Contributors (Apache-2.0)',
                 description: 'Complete offline English & Latin OCR pipeline.',
                 sizeBytes: 10665748,
                 sha256:
                     '5ca59a096cf493d5fd5044b9ccc1bdea9229fbcf656bd76040ff80a00201ee16',
                 state: ModelInstallStateId.ready,
                 installed: true,
               ),
             ],
       );

  @override
  Future<void> initialize() async {}

  @override
  String getCoreVersion() => '0.2.0-test';

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
  Future<FileInspectionModel> inspectFile(String path) async {
    if (mockInspections.containsKey(path)) {
      return mockInspections[path]!;
    }
    final ext = p.extension(path).replaceFirst('.', '').toLowerCase();
    final kind = switch (ext) {
      'docx' || 'xlsx' || 'pptx' => 'office_coming_soon',
      'csv' => 'csv',
      'json' => 'json',
      'md' => 'markdown',
      _ => 'text',
    };
    return FileInspectionModel(
      path: path,
      fileName: p.basename(path),
      extension: ext,
      previewKind: kind,
      sizeBytes: 1024,
      metadata: [
        MetadataItemModel(label: 'File Name', value: p.basename(path)),
        const MetadataItemModel(label: 'Size', value: '1.0 KB (1024 bytes)'),
      ],
    );
  }

  @override
  Future<TextPreviewModel> readTextPreview(
    String path, {
    int maxBytes = 262144,
  }) async {
    if (mockTextPreviews.containsKey(path)) {
      return mockTextPreviews[path]!;
    }
    return const TextPreviewModel(
      text: 'Sample text preview',
      truncated: false,
      totalBytes: 19,
      lineCountEstimate: 1,
    );
  }

  @override
  Future<Uint8List> renderDecodedImagePreview(
    String path, {
    int maxDimension = 2048,
  }) async {
    return Uint8List(0);
  }

  @override
  Future<Uint8List> renderPdfPagePreview(
    String path, {
    required int pageIndex,
    int targetWidth = 1100,
    String? password,
  }) async {
    return Uint8List(0);
  }

  @override
  Future<int> getPdfPageCount(String path, {String? password}) async => 1;

  AiModelsCatalogModel _buildFakeCatalog() {
    return AiModelsCatalogModel(
      schemaVersion: 1,
      settings: AiSettingsModel(
        modelsDir: _modelsDir,
        isCustomDir: _isCustomDir,
        devicePreference: _devicePref,
        availableProviders: const ['cpu', 'cuda'],
      ),
      models: List<AiModelStatusModel>.from(mockAiModels),
      flaggedExcluded: const [
        FlaggedExcludedModelInfo(
          name: 'BRIA RMBG-1.4 & RMBG-2.0',
          upstreamLicense: 'Non-Commercial',
          reason: 'Commercial restriction; excluded.',
        ),
      ],
    );
  }

  @override
  Future<AiModelsCatalogModel> getAiCatalog() async => _buildFakeCatalog();

  @override
  Stream<AiModelsCatalogModel> subscribeAiCatalog() => _aiEvents.stream;

  @override
  Future<void> startAiModelDownload(String modelId) async {
    mockAiModels = mockAiModels.map((m) {
      if (m.id != modelId) return m;
      return AiModelStatusModel(
        id: m.id,
        displayName: m.displayName,
        task: m.task,
        defaultForTask: m.defaultForTask,
        language: m.language,
        scale: m.scale,
        spdxLicense: m.spdxLicense,
        sourceUrl: m.sourceUrl,
        attribution: m.attribution,
        description: m.description,
        sizeBytes: m.sizeBytes,
        sha256: m.sha256,
        files: m.files,
        state: ModelInstallStateId.ready,
        installed: true,
        downloadedBytes: m.sizeBytes,
        progress: 1.0,
      );
    }).toList();
    _aiEvents.add(_buildFakeCatalog());
  }

  @override
  bool cancelAiModelDownload(String modelId) => true;

  @override
  Future<String> setAiModelsDir(String? customDir) async {
    _isCustomDir = customDir != null && customDir.isNotEmpty;
    _modelsDir = _isCustomDir ? customDir! : '/tmp/tool_center_ai_models';
    _aiEvents.add(_buildFakeCatalog());
    return _modelsDir;
  }

  @override
  void setAiDevicePreference(String preference) {
    _devicePref = preference;
    _aiEvents.add(_buildFakeCatalog());
  }

  @override
  Future<String> importAiModel({
    required String modelId,
    required String sourcePath,
  }) async {
    await startAiModelDownload(modelId);
    return '$_modelsDir/$modelId';
  }

  @override
  Future<bool> deleteAiModel(String modelId) async {
    mockAiModels = mockAiModels.map((m) {
      if (m.id != modelId) return m;
      return AiModelStatusModel(
        id: m.id,
        displayName: m.displayName,
        task: m.task,
        defaultForTask: m.defaultForTask,
        language: m.language,
        scale: m.scale,
        spdxLicense: m.spdxLicense,
        sourceUrl: m.sourceUrl,
        attribution: m.attribution,
        description: m.description,
        sizeBytes: m.sizeBytes,
        sha256: m.sha256,
        files: m.files,
        state: ModelInstallStateId.notInstalled,
        installed: false,
      );
    }).toList();
    _aiEvents.add(_buildFakeCatalog());
    return true;
  }
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
