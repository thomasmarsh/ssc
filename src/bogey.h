// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_BOGEY_H
#define SSC_BOGEY_H

#include "explode.h"
#include "flock.h"
#include "missile.h"
#include "shield.h"

const unsigned int MAX_BOGEYS = 400;
const unsigned int BOGEY_SHIELD_RADIUS = 13;
const unsigned int BOGEY_RADIUS = 10;
const unsigned int BOGEY_MAX_SPEED = 7;
const double BOGEY_MASS = 10.0;
const unsigned int BOGEY_FIRE_RATE = 2; // shots per second
const unsigned int BOGEY_NUM_EYES = 2;

class Bogey : public FlockMember {
public:
    Bogey(double x, double y, double z = 0);
    ~Bogey();

    static unsigned int num_alive;

    void draw();
    void fire(double dt);
    void move(double dt);
    void accelerate(double dt);

    bool collision(ScreenObject& obj);
    void damage(double amt);

    void rotate(double amt);
    void sync();

    double rx, ry;
    Shield shield;
    Explosion<20> explosion;
    Ray* mRay[BOGEY_NUM_EYES];

    float lr, lg, lb;
    double lx[BOGEY_NUM_EYES], ly[BOGEY_NUM_EYES];
};

#endif // SSC_BOGEY_H
