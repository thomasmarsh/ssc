// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "object.h"
#include "common.h"
#include "global.h"
#include "model.h"

bool ScreenObject::onScreen()
{
    return ((square(mPosition.x - Global::ship->mPosition.x) + square(mPosition.y - Global::ship->mPosition.y))
        <= square(600));
}

ScreenObject::ScreenObject(ObjectType t,
    int nradius, double nmass, double ms,
    double x, double y, double z,
    double fx, double fy, double fz)
    : PhysicsObject(nradius, nmass, x, y, z, fx, fy, fz, (void*)this)
    , rotation(0)
    , radius(nradius)
    , mFlocking(false)
    , mGravity(false)
    , mGravityAffect(0, 0)
    , mType(t)
    , mState(CREATE)
    , mMaxSpeed(ms)
    , next(0)
    , prev(0)
    , mDecelFlag(false)
{
    Model::getInstance()->addObject(this);
    reset();
}

void ScreenObject::behave() {}

// ------------------------------------------------------------------
//
// CTOR & DTOR
//
// ------------------------------------------------------------------

ScreenObject::~ScreenObject()
{
}

void ScreenObject::accelerate(double dt)
{
    double fx = sin(rotation) * dt * 10,
           fy = -cos(rotation) * dt * 10;

    PhysicsObject::accelerate(fx, fy, 0);
}

void ScreenObject::decelerate(double dt)
{
    double rot = atan2(mVelocity.x, -mVelocity.y) + M_PI;
    double tmp = rotation;
    rotation = rot;
    mDecelFlag = true;
    accelerate(speed * .05 * dt);
    mDecelFlag = false;
    rotation = tmp;
}

void ScreenObject::move(double dt)
{
    double amt = mass * mVelocity.z * dt * .1;
    if (mVelocity.z < 0)
        PhysicsObject::accelerate(0.0, 0.0, -amt);
    if (mVelocity.z > 0)
        PhysicsObject::accelerate(0.0, 0.0, amt);
    if (speed > mMaxSpeed)
        decelerate(dt);
    reset();
}

void ScreenObject::rotate(double dir)
{
    rotation += dir;
    while (rotation > D_PI)
        rotation -= D_PI;
    while (rotation < 0)
        rotation += D_PI;
}

void ScreenObject::reset()
{
    mGravityAffect.set(0, 0);
}
