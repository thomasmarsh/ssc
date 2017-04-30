// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_STARFIELD_H
#define SSC_STARFIELD_H

#include "coord.h"

const unsigned int STAR_SCALING = 70;

struct Star
{
        Coord3<unsigned int> pos;
        float r, g, b, a;
};

class StarField
{
public:
        StarField() { init(); }
        ~StarField() {}

        void draw();

private:
        void init();
        StarField(StarField&);
};

#endif // SSC_STARFIELD_H
