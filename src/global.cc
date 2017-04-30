// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "global.h"
#include "draw.h"

namespace Global
{
        Audio* audio=0;
        Ship* ship=0;
        Coord3<double> normal;
}

namespace draw
{
        DrawMode mDrawMode = DRAW_3D;
        GLUquadricObj *mSphere = gluNewQuadric();
}
