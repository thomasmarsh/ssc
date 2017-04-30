// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_GRAPH_H
#define SSC_GRAPH_H

class GraphicContext {
public:
    GraphicContext();
    ~GraphicContext();

    bool createWindow(const char* title, int w, int h, int b, bool fullscreen);
    void closeWindow();
    void resizeScene(int w, int h);

private:
    void initGL();
};

#endif // SSC_GRAPH_H
