#pragma once
#import <Cocoa/Cocoa.h>

#import "TCWBridge.h"

@class TCWWindowController;

@interface TCWWindowView : NSView

- (id)initWithController:(TCWWindowController *)_controller;
- (void)setCursorShape:(TCW3CursorShape)shape;

@end
