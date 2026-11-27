import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart'
    hide GlobalMaterialLocalizations, GlobalCupertinoLocalizations;
import 'package:material_ui/material_ui.dart';

import 'l10n/app_localizations.dart';
import 'src/services/core_service.dart';
import 'src/state/app_controller.dart';
import 'src/ui/theme/app_theme.dart';
import 'src/ui/widgets/ai_models_dialog.dart';
import 'src/ui/widgets/command_palette_dialog.dart';
import 'src/ui/widgets/home_view.dart';
import 'src/ui/widgets/jobs_drawer_panel.dart';
import 'src/ui/widgets/preview_view.dart';
import 'src/ui/widgets/tool_workspace_view.dart';

void main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final controller = AppController(backend: RustBridgeBackend());
  await controller.initialize(launchArgs: args);
  runApp(ToolCenterApp(controller: controller));
}

class ToolCenterApp extends StatefulWidget {
  final AppController? controller;

  const ToolCenterApp({super.key, this.controller});

  @override
  State<ToolCenterApp> createState() => _ToolCenterAppState();
}

class _ToolCenterAppState extends State<ToolCenterApp> {
  late final AppController _controller;
  late final bool _ownsController;

  @override
  void initState() {
    super.initState();
    if (widget.controller != null) {
      _controller = widget.controller!;
      _ownsController = false;
    } else {
      _controller = AppController(backend: FakeToolCenterBackend())
        ..initialize();
      _ownsController = true;
    }
  }

  @override
  void dispose() {
    if (_ownsController) {
      _controller.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: _controller,
      builder: (context, _) {
        return MaterialApp(
          title: 'Tool Center',
          debugShowCheckedModeBanner: false,
          themeMode: _controller.themeMode,
          theme: AppTheme.light(),
          darkTheme: AppTheme.dark(),
          localizationsDelegates: const [
            AppLocalizations.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
          ],
          supportedLocales: AppLocalizations.supportedLocales,
          home: _ToolCenterShell(controller: _controller),
        );
      },
    );
  }
}

class _ToolCenterShell extends StatelessWidget {
  final AppController controller;

  const _ToolCenterShell({required this.controller});

  Future<void> _openPalette(BuildContext context) async {
    final chosen = await showCommandPaletteDialog(
      context: context,
      tools: controller.tools,
    );
    if (chosen != null) {
      controller.openTool(chosen);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final activeCount = controller.activeJobCount;

    return Shortcuts(
      shortcuts: const <ShortcutActivator, Intent>{
        SingleActivator(LogicalKeyboardKey.keyK, control: true):
            _OpenPaletteIntent(),
        SingleActivator(LogicalKeyboardKey.keyK, meta: true):
            _OpenPaletteIntent(),
        SingleActivator(LogicalKeyboardKey.escape): _GoHomeIntent(),
      },
      child: Actions(
        actions: <Type, Action<Intent>>{
          _OpenPaletteIntent: CallbackAction<_OpenPaletteIntent>(
            onInvoke: (_) {
              _openPalette(context);
              return null;
            },
          ),
          _GoHomeIntent: CallbackAction<_GoHomeIntent>(
            onInvoke: (_) {
              if (Navigator.of(context).canPop()) {
                return null;
              }
              controller.goHome();
              return null;
            },
          ),
        },
        child: Focus(
          autofocus: true,
          child: Scaffold(
            appBar: AppBar(
              titleSpacing: 20,
              title: InkWell(
                borderRadius: BorderRadius.circular(8),
                onTap: controller.goHome,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 4,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.handyman_rounded,
                        color: theme.colorScheme.primary,
                      ),
                      const SizedBox(width: 10),
                      Text(
                        l10n.appTitle,
                        style: const TextStyle(fontWeight: FontWeight.w800),
                      ),
                    ],
                  ),
                ),
              ),
              actions: [
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 280),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: TextField(
                      decoration: InputDecoration(
                        prefixIcon: const Icon(Icons.search_rounded, size: 18),
                        hintText: l10n.searchToolsHint,
                      ),
                      onChanged: (q) {
                        if (controller.activeTool != null ||
                            controller.previewFilePath != null) {
                          controller.goHome();
                        }
                        controller.setSearchQuery(q);
                      },
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                IconButton(
                  tooltip: 'Command Palette (Ctrl+K)',
                  icon: const Icon(Icons.terminal_rounded),
                  onPressed: () => _openPalette(context),
                ),
                TextButton.icon(
                  onPressed: () => showAiModelsDialog(
                    context: context,
                    backend: controller.backend,
                  ),
                  icon: const Icon(Icons.auto_awesome_outlined, size: 18),
                  label: Text(l10n.aiModelsButton),
                ),
                Badge(
                  isLabelVisible: activeCount > 0,
                  label: Text('$activeCount'),
                  child: IconButton(
                    tooltip: l10n.jobsPanelTitle,
                    icon: const Icon(Icons.bolt_rounded),
                    onPressed: controller.toggleJobsDrawer,
                  ),
                ),
                IconButton(
                  tooltip: l10n.themeTooltip,
                  icon: Icon(switch (controller.themeMode) {
                    ThemeMode.light => Icons.light_mode_outlined,
                    ThemeMode.dark => Icons.dark_mode_outlined,
                    ThemeMode.system => Icons.brightness_auto_outlined,
                  }),
                  onPressed: controller.cycleThemeMode,
                ),
                const SizedBox(width: 12),
              ],
            ),
            body: Row(
              children: [
                Expanded(child: _buildActiveBody()),
                if (controller.jobsDrawerOpen)
                  JobsDrawerPanel(controller: controller),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildActiveBody() {
    if (controller.previewFilePath != null) {
      return FilePreviewView(
        controller: controller,
        filePath: controller.previewFilePath!,
      );
    }
    if (controller.activeTool != null) {
      return ToolWorkspaceView(
        controller: controller,
        tool: controller.activeTool!,
        initialFiles: controller.activeToolInitialFiles,
      );
    }
    return HomeView(controller: controller);
  }
}

class _OpenPaletteIntent extends Intent {
  const _OpenPaletteIntent();
}

class _GoHomeIntent extends Intent {
  const _GoHomeIntent();
}
