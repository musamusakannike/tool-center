import 'package:material_ui/material_ui.dart';

import '../../state/app_controller.dart';
import 'models_view.dart';

/// Opens the AI Models manager inside a modal dialog when needed from sub-contexts.
Future<void> showAiModelsDialog({
  required BuildContext context,
  required AppController controller,
}) {
  return showDialog<void>(
    context: context,
    builder: (ctx) => Dialog.fullscreen(
      child: Scaffold(
        body: ModelsView(controller: controller),
      ),
    ),
  );
}
