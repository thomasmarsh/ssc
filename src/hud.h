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

class HUD
{
public:
        inline static HUD* getInstance()
        {
                if (!hud) hud = new HUD();
                return hud;
        }

        static void init()
        {
                getInstance()->initialize();
        }

        void toggleRadar() { mShowRadar = !mShowRadar; }
        void update(Ship*);
        void draw();

private:
        Ship *ship;
        bool mShowRadar;

        void initialize();

        //  -- statics --

        static HUD *hud;

        static const char* SPEED_TEXT;
        static const char* ANGLE_TEXT;

        HUD();
};

#endif // SSC_HUD_H
