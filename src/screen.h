// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_SCREEN_H
#define SSC_SCREEN_H

#include "coord.h"
#include "config.h"

class Screen
{
public:
        static inline int maxX() { return mGameArea.x; }
        static inline int maxY() { return mGameArea.y; }

        static inline double fieldOfView() { return fov; }
        static inline double zNear() { return zn; }
        static inline double zFar() { return zf; }

        static inline double aspectRatio()
        {
                return ((double) mDisplay.x / (double) mDisplay.y);
        }

        static inline void init()
        {
                Config* conf = Config::getInstance();
                mDisplay = conf->getResolution();
                mGameArea = conf->getGameArea();
                fov = conf->getFOV();
                zn = conf->getZNear();
                zf = conf->getZFar();
        }

        static Coord2<int> mDisplay, mGameArea;
        static double fov, zn, zf;
};

#define SX(o)   ((o)->mPosition.x)
#define SY(o)   ((o)->mPosition.y)

#endif // SSC_SCREEN_H
