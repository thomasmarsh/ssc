// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_GLOBAL_H
#define SSC_GLOBAL_H

#include "audio.h"
#include "missile.h"
#include "ship.h"

namespace Global {
extern Audio* audio;
extern Ship* ship;
extern Coord3<double> normal;
}

#endif // SSC_GLOBAL_H
