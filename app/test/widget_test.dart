import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:tool_center/main.dart';
import 'package:tool_center/src/models/preview_models.dart';
import 'package:tool_center/src/models/tool_models.dart';
import 'package:tool_center/src/services/core_service.dart';
import 'package:tool_center/src/state/app_controller.dart';

void main() {
  setUp(() {
    SharedPreferences.setMockInitialValues({});
  });

  final sampleTool = ToolDescriptorModel(
    id: 'image.resize',
    name: 'Resize Image',
    nameKey: 'tool_image_resize',
    description: 'Change width and height smoothly.',
    descriptionKey: 'tool_image_resize_desc',
    category: ToolCategoryId.images,
    icon: 'resize',
    keywords: const ['scale', 'dimensions', 'png', 'jpg'],
    inputSpec: const ToolInputSpecModel(
      acceptedExtensions: ['png', 'jpg'],
      minFiles: 1,
      maxFiles: null,
      allowDirectories: false,
    ),
    optionsSchema: const [
      OptionFieldModel(
        key: 'width',
        label: 'Target Width',
        labelKey: 'opt_width',
        hint: 'Width in pixels.',
        hintKey: 'opt_width_hint',
        kindType: OptionKindType.integer,
        min: 1,
        max: 16000,
        unit: 'px',
        defaultValue: 1280,
      ),
    ],
    defaultSuffix: '_resized',
  );

  testWidgets(
    'Home grid renders tools, Command Palette opens with Ctrl+K, and schema form renders',
    (WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 860);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);

      final backend = FakeToolCenterBackend(tools: [sampleTool]);
      final controller = AppController(backend: backend);
      await controller.initialize();

      await tester.pumpWidget(ToolCenterApp(controller: controller));
      await tester.pumpAndSettle();

      expect(find.text('Tool Center'), findsWidgets);
      expect(find.text('Resize Image'), findsOneWidget);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(
        find.text('Type a tool name, format, or action...'),
        findsOneWidget,
      );

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();

      expect(find.text('Target Width'), findsOneWidget);
      expect(find.text('Width in pixels.'), findsOneWidget);
    },
  );

  testWidgets(
    'Preview renders DOCX coming-soon fallback card, CSV table, and metadata sidebar',
    (WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 860);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);

      final backend = FakeToolCenterBackend(
        tools: [sampleTool],
        mockTextPreviews: {
          '/tmp/metrics.csv': const TextPreviewModel(
            text: 'city,population\nTokyo,37400000\nLagos,15900000',
            truncated: false,
            totalBytes: 46,
            lineCountEstimate: 3,
            csvHeaders: ['city', 'population'],
            csvRows: [
              ['Tokyo', '37400000'],
              ['Lagos', '15900000'],
            ],
          ),
        },
      );
      final controller = AppController(backend: backend);
      await controller.initialize();

      await tester.pumpWidget(ToolCenterApp(controller: controller));
      await tester.pumpAndSettle();

      // DOCX fallback
      controller.openPreview('/tmp/quarterly.docx');
      await tester.pumpAndSettle();
      expect(find.text('Preview coming soon'), findsOneWidget);
      expect(find.text('Open in System App'), findsWidgets);
      expect(find.text('File Metadata'), findsOneWidget);

      // CSV table preview
      controller.openPreview('/tmp/metrics.csv');
      await tester.pumpAndSettle();
      expect(find.text('Tokyo'), findsOneWidget);
      expect(find.text('Lagos'), findsOneWidget);
    },
  );

  testWidgets(
    'Models screen displays size before downloading and AI tool shows one-click Download Model prompt',
    (WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);

      const sampleAiTool = ToolDescriptorModel(
        id: 'ai.remove_bg',
        name: 'Remove Background',
        nameKey: 'tool_ai_remove_bg',
        description: 'Cut out subjects cleanly offline.',
        descriptionKey: 'tool_ai_remove_bg_desc',
        category: ToolCategoryId.ai,
        icon: 'auto_fix_high',
        keywords: ['matting', 'cutout'],
        inputSpec: ToolInputSpecModel(
          acceptedExtensions: ['png', 'jpg'],
          minFiles: 1,
          maxFiles: null,
          allowDirectories: false,
        ),
        optionsSchema: [
          OptionFieldModel(
            key: 'model_id',
            label: 'AI Model',
            labelKey: 'opt_ai_model',
            hint: 'Choose fast or high-detail matting.',
            hintKey: 'opt_ai_model_hint',
            kindType: OptionKindType.select,
            choices: [
              SelectChoiceModel(
                value: 'bg.u2netp',
                label: 'U²-Netp (Fast)',
              ),
            ],
            defaultValue: 'bg.u2netp',
          ),
        ],
        defaultSuffix: '_nobg',
      );

      final backend = FakeToolCenterBackend(tools: [sampleTool, sampleAiTool]);
      final controller = AppController(backend: backend);
      await controller.initialize();

      await tester.pumpWidget(ToolCenterApp(controller: controller));
      await tester.pumpAndSettle();

      // 1. Open AI tool whose model (`bg.u2netp`) is not yet installed -> shows exact size prompt!
      controller.openTool(sampleAiTool, initialFiles: const ['/tmp/photo.png']);
      await tester.pumpAndSettle();
      expect(
        find.text('Model required: U²-Netp (Fast General & Portrait)'),
        findsOneWidget,
      );
      expect(find.text('Download Model (4.4 MB)'), findsOneWidget);

      // 2. Open dedicated Models screen -> shows sizes, statuses, and Install/Remove buttons
      await tester.tap(find.text('Models'));
      await tester.pumpAndSettle();
      expect(find.text('AI Models & Runtime'), findsOneWidget);
      expect(find.text('Download (4.4 MB)'), findsOneWidget);
      expect(find.text('Remove'), findsOneWidget);

      // 3. Click Download (4.4 MB) -> transitions to Ready!
      await tester.tap(find.text('Download (4.4 MB)'));
      await tester.pumpAndSettle();
      expect(find.text('Download (4.4 MB)'), findsNothing);
    },
  );
}
