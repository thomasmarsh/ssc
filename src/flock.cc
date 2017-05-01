// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "flock.h"
#include "draw.h"
#include "global.h"
#include "model.h"
#include "object.h"

const unsigned int FLOCK_MAX_MEMBERS = 20;
const unsigned int FLOCK_MIN_MEMBERS = 3;
static const double MAX_TARGET_AGE = 50 * 1000;

// --------------------------------------------------------------------------
//
// FlockMember method implementations
//
// --------------------------------------------------------------------------

FlockMember::FlockMember(ObjectType type,
                         int radius, double mass, double ms,
                         double x, double y, double z,
                         double fx, double fy, double fz)
    : ScreenObject(type, radius, mass, ms, x, y, z, fx, fy, fz)
    , mFlock(0)
    , mAvoidance(0, 0, 0)
    , mAvoidCount(0)
{
    mFlocking = true;
}

void FlockMember::disband()
{
    if (mFlock) {
        mFlock->disband();
        if (mFlock->mNumMembers == 0) {
            delete mFlock;
        }
        mFlock = 0;
    }
}

FlockMember::~FlockMember()
{
    disband();
}

void FlockMember::setState(ObjectState state)
{
    // remove from flock if dying
    if (isDying() || isDead()) {
        disband();
    }

    ScreenObject::setState(state);
}

void FlockMember::interact(ScreenObject& other)
{
    // have to do a C-style cast to placate Windows
    FlockMember& fm = *((FlockMember*)other.getAddress());
    initiateFlocking(fm);
}

void FlockMember::initiateFlocking(FlockMember& other)
{
    // both already have a flock
    if (mFlock && other.mFlock) {
        if ((mFlock->mNumMembers > other.mFlock->mNumMembers) || (mFlock->mMaxMembers > other.mFlock->mMaxMembers)) {
            if (!mFlock->isFull()) {
                other.mFlock->disband();
                other.mFlock = mFlock;
                mFlock->addMember();
            }
        } else if ((other.mFlock->mNumMembers > mFlock->mNumMembers) || (other.mFlock->mMaxMembers > mFlock->mMaxMembers)) {
            if (!other.mFlock->isFull()) {
                mFlock->disband();
                mFlock = other.mFlock;
                mFlock->addMember();
            }
        }
        return;
    }

    // the other object has a flock, call again using its method
    if (!mFlock && other.mFlock) {
        other.initiateFlocking(*this);
        return;
    }

    // construct a new flock if we don't have one
    if (!mFlock) {
        mFlock = new Flock();
        mFlock->addMember();
    }

    // add partner if this flock isn't full
    if (!mFlock->isFull()) {
        mFlock->addMember();
        other.mFlock = mFlock;
    }
}

void FlockMember::reset() { ScreenObject::reset(); }

class FlockAccum {
public:
    FlockAccum()
        : count(0)
        , fx(0)
        , fy(0)
    {
    }

    inline void reset()
    {
        count = 0;
        fx = fy = 0;
    }

    inline void accumulate(Coord3<double>& a,
                           Coord3<double>& b,
                           int weight)
    {
        Coord3<double> t;

        t = a - b;
        if (t.x > 1) {
            fx += (1 - (1 / t.x)) * weight;
        } else if (t.x < -1) {
            fx -= (1 - (1 / -t.x)) * weight;
        }

        if (t.y > 1) {
            fy += (1 - (1 / t.y)) * weight;
        } else if (t.y < -1) {
            fy -= (1 - (1 / -t.y)) * weight;
        }
        count += weight;
    }

    inline void finalize(double maxspeed)
    {
        fx /= count;
        fy /= count;
    }

    inline double getFX() { return fx; }
    inline double getFY() { return fy; }

private:
    int count;
    double fx, fy;
};

void FlockMember::move(double dt)
{
    ScreenObject::move(dt);

    if (mFlock) {
        mFlock->updateCentroid(*this);
    }

    // we need at least another member to flock
    if (mFlock && mFlock->mNumMembers == 1) {
        disband();
    }

    // bailout if not in a flock
    if (!mFlock) {
        return;
    }

    // check if random target has expired
    mFlock->checkTarget(dt);

    // decelerate if going too fast
    speed = hypot(mVelocity.x, mVelocity.y);
    if (speed > mMaxSpeed) {
        decelerate(dt);
    }

    // A new force vector
    static Coord3<double> rndDir;
    rndDir.set((rand() % 5) - 2,
               (rand() % 5) - 2, 0);

    static FlockAccum ac;
    ac.reset();

    const int F_HEADING = 10;
    const int F_CENTROID = 30;
    const int F_RANDOM = 50;
    const int F_SHIP = 70;
    const int F_RND = 4;
    const int F_AVOID = 100;

    ac.accumulate(mFlock->mCenterForce, mVelocity, F_HEADING);
    ac.accumulate(mFlock->mCenterPos, mPosition, F_CENTROID);
    ac.accumulate(mFlock->mTarget, mPosition, F_RANDOM);
    ac.accumulate(rndDir, mVelocity, F_RND);

    if (mAvoidCount) {
        static Coord3<double> tmp;
        tmp = mAvoidance / mAvoidCount;
        ac.accumulate(tmp, mVelocity, F_AVOID);
        mAvoidance.set(0, 0, 0);
        mAvoidCount = 0;
    }

    if (Global::ship->isAlive() && onScreen()) {
        ac.accumulate(Global::ship->mPosition, mPosition, F_SHIP);
    }

    ac.finalize(mMaxSpeed);
    double fx = ac.getFX();
    double fy = ac.getFY();
    rotation = atan2(fx, -fy);
    speed = hypot(fx, fy);
    accelerate(speed * dt);
}

// --------------------------------------------------------------------------
//
// Flock method implementations
//
// --------------------------------------------------------------------------

Flock::Flock()
    : r(((float)(rand() % 100000)) / 100000.0f)
    , g(((float)(rand() % 100000)) / 100000.0f)
    , b(((float)(rand() % 100000)) / 100000.0f)
    , mTarget(0, 0, 0)
    , mTargetAge((double)(rand() % (int)MAX_TARGET_AGE))
    , mMaxMembers(((rand() % (FLOCK_MAX_MEMBERS - FLOCK_MIN_MEMBERS - 1))
                   + FLOCK_MIN_MEMBERS))
    , mNumMembers(0)
    , mMembersUpdated(0)
{
    setTarget();
}

Flock::~Flock()
{
}
void Flock::setTarget()
{
    mTarget.set(rand() % Screen::maxX(), rand() % Screen::maxY(), 0);
    mTargetAge = 0;
}

void Flock::checkTarget(double dt)
{
    if (mMembersUpdated > 0) {
        return;
    }

    mTargetAge += dt * mNumMembers;

    if ((mTargetAge > MAX_TARGET_AGE) || ((square(mTarget.x - mCenterPos.x) + square(mTarget.y - mCenterPos.y)) <= 1000)) {
        setTarget();
    }
}

void Flock::updateCentroid(FlockMember& obj)
{
    double mag = obj.mVelocity.dot(obj.mVelocity);
    Coord3<double> oForce(obj.mVelocity),
        oPos(obj.mPosition);

    // reset centroid
    if (mMembersUpdated == 0) {
        mTmpCenterForce.set(0, 0, 0);
        mTmpCenterPos.set(0, 0, 0);
    }

    if (mag > 25)
        mTmpCenterForce += oForce / mag * 25;
    else
        mTmpCenterForce += oForce;

    mTmpCenterPos += oPos;

    mMembersUpdated++;

    // average centroid
    if (mMembersUpdated == mNumMembers) {
        mCenterForce = mTmpCenterForce / (double)mNumMembers;
        mCenterPos = mTmpCenterPos / (double)mNumMembers;
        mMembersUpdated = 0;
    }
}
