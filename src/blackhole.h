// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_BLACKHOLE_H
#define SSC_BLACKHOLE_H

#include "object.h"

const unsigned int BLACKHOLE_SPEED = 7;
const unsigned int BLACKHOLE_RADIUS = 10;
const double BLACKHOLE_MASS = 100.0;

class BlackHole : public ScreenObject
{
private:
        BlackHole(BlackHole&);
public:

        BlackHole(double x, double y);

        ~BlackHole();

        using ScreenObject::move;

        void move() {}
        bool collision(ScreenObject&) { return false; }
        void bounce(double, double, double, int, double) {}
        void draw();
};


#endif // SSC_BLACKHOLE_H
