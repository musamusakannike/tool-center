import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../models/tool_models.dart';
import '../services/core_service.dart';

class AppController extends ChangeNotifier {
  static const _prefsThemeKey = 'tool_center.theme_mode';
  static const _prefsRecentKey = 'tool_center.recent_files';

  final ToolCenterBackend backend;

  ThemeMode _themeMode = ThemeMode.system;
  List<ToolDescriptorModel> _tools = const [];
  final List<JobSnapshotModel> _jobs = [];
  List<String> _recentFiles = [];
  ToolCategoryId? _selectedCategory;
  String _searchQuery = '';
  ToolDescriptorModel? _activeTool;
  List<String> _activeToolInitialFiles = const [];
  String? _previewFilePath;
  bool _jobsDrawerOpen = false;
  bool _ready = false;
  String? _startupError;
  StreamSubscription<JobSnapshotModel>? _jobEventsSub;

  AppController({required this.backend});

  ThemeMode get themeMode => _themeMode;
  List<ToolDescriptorModel> get tools => _tools;
  List<JobSnapshotModel> get jobs => List.unmodifiable(_jobs);
  List<String> get recentFiles => List.unmodifiable(_recentFiles);
  ToolCategoryId? get selectedCategory => _selectedCategory;
  String get searchQuery => _searchQuery;
  ToolDescriptorModel? get activeTool => _activeTool;
  List<String> get activeToolInitialFiles => _activeToolInitialFiles;
  String? get previewFilePath => _previewFilePath;
  bool get jobsDrawerOpen => _jobsDrawerOpen;
  bool get ready => _ready;
  String? get startupError => _startupError;

  int get activeJobCount => _jobs.where((j) => j.status.isActive).length;

  List<ToolDescriptorModel> get filteredTools {
    return _tools.where((tool) {
      if (_selectedCategory != null && tool.category != _selectedCategory) {
        return false;
      }
      return tool.matchesQuery(_searchQuery);
    }).toList();
  }

  Future<void> initialize({List<String> launchArgs = const []}) async {
    try {
      await _loadPreferences();
      await backend.initialize();
      _tools = backend.listTools();
      _jobs
        ..clear()
        ..addAll(backend.listJobs());
      _jobEventsSub = backend.subscribeJobSnapshots().listen(_onJobSnapshot);
      _ready = true;

      // Support OS "Open With" CLI arguments
      for (final arg in launchArgs) {
        if (arg.trim().isNotEmpty && !arg.startsWith('-')) {
          final file = File(arg);
          if (file.existsSync()) {
            openPreview(file.path);
            break;
          }
        }
      }
    } catch (e) {
      _startupError = e.toString();
      _ready = true;
    }
    notifyListeners();
  }

  Future<void> _loadPreferences() async {
    try {
      final prefs = await SharedPreferences.getInstance().timeout(
        const Duration(milliseconds: 250),
      );
      final rawTheme = prefs.getString(_prefsThemeKey);
      if (rawTheme == 'light') {
        _themeMode = ThemeMode.light;
      } else if (rawTheme == 'dark') {
        _themeMode = ThemeMode.dark;
      } else {
        _themeMode = ThemeMode.system;
      }
      _recentFiles = prefs.getStringList(_prefsRecentKey) ?? [];
    } catch (_) {
      // Non-fatal if preferences are unavailable in headless environments
    }
  }

  Future<void> cycleThemeMode() async {
    switch (_themeMode) {
      case ThemeMode.system:
        _themeMode = ThemeMode.light;
      case ThemeMode.light:
        _themeMode = ThemeMode.dark;
      case ThemeMode.dark:
        _themeMode = ThemeMode.system;
    }
    notifyListeners();
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_prefsThemeKey, _themeMode.name);
    } catch (_) {}
  }

  void setCategory(ToolCategoryId? category) {
    _selectedCategory = category;
    notifyListeners();
  }

  void setSearchQuery(String query) {
    _searchQuery = query;
    notifyListeners();
  }

  void openTool(
    ToolDescriptorModel tool, {
    List<String> initialFiles = const [],
  }) {
    _previewFilePath = null;
    _activeTool = tool;
    _activeToolInitialFiles = List<String>.from(initialFiles);
    for (final path in initialFiles) {
      recordRecentFile(path);
    }
    notifyListeners();
  }

  void openPreview(String filePath) {
    _activeTool = null;
    _activeToolInitialFiles = const [];
    _previewFilePath = filePath;
    recordRecentFile(filePath);
    notifyListeners();
  }

  void goHome() {
    _activeTool = null;
    _activeToolInitialFiles = const [];
    _previewFilePath = null;
    notifyListeners();
  }

  void toggleJobsDrawer([bool? open]) {
    _jobsDrawerOpen = open ?? !_jobsDrawerOpen;
    notifyListeners();
  }

  Future<void> recordRecentFile(String filePath) async {
    if (filePath.trim().isEmpty) return;
    _recentFiles.remove(filePath);
    _recentFiles.insert(0, filePath);
    if (_recentFiles.length > 20) {
      _recentFiles = _recentFiles.sublist(0, 20);
    }
    notifyListeners();
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setStringList(_prefsRecentKey, _recentFiles);
    } catch (_) {}
  }

  Future<void> clearRecentFiles() async {
    _recentFiles.clear();
    notifyListeners();
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(_prefsRecentKey);
    } catch (_) {}
  }

  Future<String> submitToolJob({
    required String toolId,
    required List<String> inputs,
    required Map<String, dynamic> options,
    String? outputDir,
  }) async {
    for (final input in inputs) {
      unawaited(recordRecentFile(input));
    }
    return backend.submitJob(
      toolId: toolId,
      inputs: inputs,
      options: options,
      outputDir: outputDir,
    );
  }

  void cancelJob(String jobId) {
    backend.cancelJob(jobId);
  }

  void clearFinishedJobs() {
    backend.clearFinishedJobs();
    _jobs.removeWhere((j) => !j.status.isActive);
    notifyListeners();
  }

  void _onJobSnapshot(JobSnapshotModel snapshot) {
    final idx = _jobs.indexWhere((j) => j.jobId == snapshot.jobId);
    if (idx >= 0) {
      _jobs[idx] = snapshot;
    } else {
      _jobs.insert(0, snapshot);
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _jobEventsSub?.cancel();
    super.dispose();
  }
}
