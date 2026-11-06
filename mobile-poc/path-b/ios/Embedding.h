#import <UIKit/UIKit.h>
#include <stdbool.h>

// Provided by this crate.
void poc_start(void);

// Provided by gpui-pre-mobile.
void gpui_ios_set_embedded(void);
void *gpui_ios_get_window(void);
void *gpui_ios_view_controller(void *window);
void gpui_ios_layout_view(void *window);
bool gpui_ios_request_frame(void *window);
void gpui_ios_did_become_active(void *app);
void gpui_ios_will_resign_active(void *app);
