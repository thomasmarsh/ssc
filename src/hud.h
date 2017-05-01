// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_HUD_H
#define SSC_HUD_H

#include "ship.h"

#include <memory>

class HUD {
public:
    inline static HUD& getInstance()
    {
        static HUD instance;
        return instance;
        //if (!hud) {
        //    hud = std::make_unique<HUD>();
        //}
        //return hud;
    }

    static void init()
    {
        getInstance().initialize();
    }

    void toggleRadar() { mShowRadar = !mShowRadar; }
    void update(Ship*);
    void draw();

private:
    Ship* ship;
    bool mShowRadar;

    void initialize();

    //  -- statics --

    //static std::unique_ptr<HUD> hud;

    static const char* SPEED_TEXT;
    static const char* ANGLE_TEXT;

    HUD();
};

#endif // SSC_HUD_H
