// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_FLOCK_H
#define SSC_FLOCK_H

#include "common.h"
#include "object.h"

class Flock;

// --------------------------------------------------------------------------
//
// FlockMember
//
// Subclass of ScreenObject
//
// --------------------------------------------------------------------------

class FlockMember : public ScreenObject {
public:
    FlockMember(ObjectType type,
        int radius, double mass, double ms,
        double x, double y, double z = 0,
        double fx = 0, double fy = 0, double fz = 0);
    virtual ~FlockMember();

    void disband();
    void avoid(Collidable* other)
    {
        if (!mFlock)
            return;
        Coord3<double> delta = mPosition - other->mPosition;
        mAvoidance += delta;
        mAvoidCount++;
    }

    void reset();
    virtual void move(double dt);
    void update(FlockMember&);
    virtual void setState(ObjectState state);
    virtual void interact(ScreenObject& other);
    void initiateFlocking(FlockMember& other);

protected:
    Flock* mFlock;
    Coord3<double> mAvoidance;
    int mAvoidCount;
};

// --------------------------------------------------------------------------
//
// Flock
//
// Responsible for maintaining centroid
//
// --------------------------------------------------------------------------

class Flock {
public:
    Flock();
    ~Flock();

    float r, g, b;
    Coord3<double> mTarget;
    double mTargetAge;
    unsigned int mMaxMembers;
    unsigned int mNumMembers;
    unsigned int mMembersUpdated;
    Coord3<double> mCenterForce, mTmpCenterForce,
        mCenterPos, mTmpCenterPos;

    inline bool isEmpty() { return (mNumMembers <= 1); }
    inline bool isFull() { return (mNumMembers >= mMaxMembers); }

    inline void addMember() { mNumMembers++; }
    inline void disband() { mNumMembers--; }

    void checkTarget(double dt);
    void setTarget();

    void updateCentroid(FlockMember&);
};

#endif // SSC_FLOCK_H
